use super::*;

// fileUpdate validates against existing file records that may only be known
// upstream. In LiveHybrid these hydration reads fetch the referenced file/product
// records before staging local updates; in replay they match the recorded
// cassette calls, and against a live backend they are ordinary GraphQL reads.
const MEDIA_FILE_UPDATE_HYDRATE_QUERY: &str = "query MediaFileUpdateHydrate($fileIds: [ID!]!) {\n  nodes(ids: $fileIds) {\n    id\n    __typename\n    ... on File {\n      alt\n      createdAt\n      fileStatus\n    }\n    ... on MediaImage {\n      image { url width height }\n      preview { image { url width height } }\n    }\n    ... on GenericFile {\n      url\n    }\n  }\n}";
pub(in crate::proxy) const MEDIA_PRODUCT_HYDRATE_QUERY: &str = "query MediaProductHydrate($id: ID!) {\n  product(id: $id) {\n    id\n    title\n    handle\n    status\n    media(first: 50) {\n      nodes {\n        id\n        alt\n        mediaContentType\n        status\n        preview { image { url width height } }\n        ... on MediaImage { image { url width height } }\n      }\n    }\n    variants(first: 50) {\n      nodes {\n        id\n        title\n        media(first: 10) { nodes { id } }\n      }\n    }\n  }\n}";
// fileDelete / fileUpdate cascade clearing needs to know which products (and
// their variants) a media file is attached to, so a delete or detach can remove
// the file id from those owners. Shopify exposes no local reverse index, so in
// LiveHybrid we read the file's `references` from upstream; in replay this
// matches the recorded cassette call. Both the product `media` nodes and each
// variant's attached `media` are hydrated so the cascade and downstream variant
// reads operate on real owner state. (Gleam parity: PR #794 file media cascade.)
const MEDIA_FILE_REFERENCES_HYDRATE_QUERY: &str = "query MediaFileReferencesHydrate($fileIds: [ID!]!) {\n  nodes(ids: $fileIds) {\n    id\n    __typename\n    ... on MediaImage {\n      alt\n      fileStatus\n      mediaContentType\n      status\n      preview { image { url width height } }\n      image { url width height }\n      references(first: 50) {\n        nodes {\n          ... on Product {\n            id\n            title\n            handle\n            status\n            media(first: 50) {\n              nodes {\n                id\n                __typename\n                alt\n                fileStatus\n                mediaContentType\n                status\n                preview { image { url width height } }\n                ... on MediaImage { image { url width height } }\n              }\n            }\n            variants(first: 50) {\n              nodes {\n                id\n                title\n                media(first: 10) { nodes { id alt mediaContentType } }\n              }\n            }\n          }\n        }\n      }\n    }\n  }\n}";

impl DraftProxy {
    pub(in crate::proxy) fn media_mutation(
        &mut self,
        root_field: &str,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
        match root_field {
            "fileCreate" => self.media_file_create(request, query, variables),
            "fileUpdate" => self.media_file_update(request, query, variables),
            "fileDelete" => self.media_file_delete(request, query, variables),
            "fileAcknowledgeUpdateFailed" => {
                self.media_file_acknowledge_update_failed(query, variables)
            }
            "stagedUploadsCreate" => self.media_staged_uploads_create(query, variables),
            _ => MutationOutcome::response(json_error(501, "Unsupported media mutation root")),
        }
    }

    pub(in crate::proxy) fn media_file_create(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
        let (response_key, payload_selection) =
            primary_root_response_selection(query, variables, || "fileCreate".to_string());
        let inputs = media_object_list_arg(query, variables, "files");
        if manage_products_denied(request) && media_inputs_have_references(&inputs) {
            return MutationOutcome::response(media_access_denied_response(
                &response_key,
                "fileCreate",
            ));
        }

        if inputs.len() > 250 {
            return MutationOutcome::response(ok_json(json!({
                "errors": [{
                    "message": format!("The input array size of {} is greater than the maximum allowed of 250.", inputs.len()),
                    "locations": [{"line": 2, "column": 3}],
                    "path": ["fileCreate", "files"],
                    "extensions": {"code": "MAX_INPUT_SIZE_EXCEEDED"}
                }]
            })));
        }

        for (index, input) in inputs.iter().enumerate() {
            match resolved_string_field(input, "originalSource") {
                None => {
                    return MutationOutcome::response(ok_json(json!({
                        "errors": [{
                            "message": format!("Variable $files of type [FileCreateInput!]! was provided invalid value for {index}.originalSource (Expected value to not be null)"),
                            "locations": [{"line": 2, "column": 43}],
                            "extensions": {
                                "code": "INVALID_VARIABLE",
                                "value": resolved_variables_json(variables).get("files").cloned().unwrap_or(Value::Null),
                                "problems": [{
                                    "path": [index, "originalSource"],
                                    "explanation": "Expected value to not be null"
                                }]
                            }
                        }]
                    })));
                }
                Some(source) if source.is_empty() => {
                    return MutationOutcome::response(media_invalid_field_arguments_response(
                        &response_key,
                        "fileCreate",
                        "originalSource is too short (minimum is 1)",
                    ));
                }
                Some(source) if source.chars().count() > 2048 => {
                    return MutationOutcome::response(media_invalid_field_arguments_response(
                        &response_key,
                        "fileCreate",
                        "originalSource is too long (maximum is 2048)",
                    ));
                }
                _ => {}
            }
        }

        let errors = inputs
            .iter()
            .enumerate()
            .filter_map(|(index, input)| validate_file_create_input(input, index))
            .chain(media_quota_errors(request, &inputs))
            .collect::<Vec<_>>();
        if !errors.is_empty() {
            let payload = json!({"files": [], "userErrors": errors});
            return MutationOutcome::response(ok_json(json!({
                "data": {response_key: selected_json(&payload, &payload_selection)}
            })));
        }

        // Each successful mutation reserves a synthetic id for its log entry
        // before allocating resource ids (Gleam fileCreate reserves a
        // MutationLogEntry id first), keeping file ids in lockstep with parity.
        self.reserve_synthetic_log_id();
        let files = inputs
            .into_iter()
            .enumerate()
            .map(|(index, input)| {
                let original_source =
                    resolved_string_field(&input, "originalSource").unwrap_or_default();
                let filename = resolved_string_field(&input, "filename")
                    .unwrap_or_else(|| filename_from_source(&original_source));
                // When contentType is omitted, Shopify infers only image/video
                // media from the source/filename extension. 3D models require
                // an explicit MODEL_3D contentType; otherwise they are files.
                let content_type = resolved_string_field(&input, "contentType")
                    .unwrap_or_else(|| infer_content_type_from_source(&filename).to_string());
                let resource_type = media_file_gid_type(&content_type);
                let id = self.next_synthetic_gid(resource_type);
                let alt = resolved_string_field(&input, "alt").unwrap_or_default();
                let created_at = file_create_timestamp_for_index(index);
                let file = media_file_record(
                    &id,
                    &content_type,
                    &filename,
                    &alt,
                    &original_source,
                    "UPLOADED",
                    &created_at,
                );
                self.store.staged.media_files.insert(id, file.clone());
                file
            })
            .collect::<Vec<_>>();
        let staged_ids = files
            .iter()
            .filter_map(|file| file.get("id").and_then(Value::as_str).map(str::to_string))
            .collect::<Vec<_>>();
        let payload = json!({"files": files, "userErrors": []});
        MutationOutcome::staged(
            ok_json(json!({"data": {response_key: selected_json(&payload, &payload_selection)}})),
            LogDraft::staged("fileCreate", "media", staged_ids),
        )
    }

    pub(in crate::proxy) fn media_file_update(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
        let (response_key, payload_selection) =
            primary_root_response_selection(query, variables, || "fileUpdate".to_string());
        let inputs = media_object_list_arg(query, variables, "files");
        if manage_products_denied(request) && media_inputs_have_references(&inputs) {
            return MutationOutcome::response(media_access_denied_response(
                &response_key,
                "fileUpdate",
            ));
        }
        // originalSource over the 2048-char argument limit is a document-level
        // coercion error (top-level errors + null payload), matching Shopify.
        for input in &inputs {
            if let Some(source) = resolved_string_field(input, "originalSource") {
                if source.chars().count() > 2048 {
                    return MutationOutcome::response(media_invalid_field_arguments_response(
                        &response_key,
                        "fileUpdate",
                        "originalSource is too long (maximum is 2048)",
                    ));
                }
            }
        }

        let required_field_errors = inputs
            .iter()
            .enumerate()
            .filter_map(|(index, input)| validate_file_update_required_fields(input, index))
            .collect::<Vec<_>>();
        if !required_field_errors.is_empty() {
            return MutationOutcome::response(media_file_update_error_response(
                &response_key,
                &payload_selection,
                required_field_errors,
            ));
        }

        // Hydrate referenced products and file-update targets from upstream so
        // existence/validation checks run against the real records (Gleam parity:
        // maybe_hydrate_referenced_products + maybe_hydrate_file_update_targets).
        self.hydrate_referenced_products(request, &inputs);
        self.hydrate_file_update_targets(request, &inputs);

        let mut missing_ids = Vec::new();
        extend_unique_strings(
            &mut missing_ids,
            inputs
                .iter()
                .filter_map(|input| resolved_string_field(input, "id"))
                .filter(|id| self.media_file_for_update(id).is_none()),
        );
        if !missing_ids.is_empty() {
            return MutationOutcome::response(media_file_update_error_response(
                &response_key,
                &payload_selection,
                vec![file_update_missing_ids_error(&missing_ids)],
            ));
        }

        let non_ready_ids = inputs
            .iter()
            .filter_map(|input| resolved_string_field(input, "id"))
            .filter(|id| {
                self.media_file_for_update(id)
                    .and_then(|file| {
                        file.get("fileStatus")
                            .and_then(Value::as_str)
                            .map(str::to_string)
                    })
                    .is_none_or(|status| status != "READY")
            })
            .collect::<Vec<_>>();
        if !non_ready_ids.is_empty() {
            return MutationOutcome::response(media_file_update_error_response(
                &response_key,
                &payload_selection,
                vec![json!({
                    "field": ["files"],
                    "message": "Non-ready files cannot be updated.",
                    "code": "NON_READY_STATE"
                })],
            ));
        }

        let post_readiness_field_errors = inputs
            .iter()
            .enumerate()
            .flat_map(|(index, input)| validate_file_update_post_readiness_fields(input, index))
            .collect::<Vec<_>>();
        if !post_readiness_field_errors.is_empty() {
            return MutationOutcome::response(media_file_update_error_response(
                &response_key,
                &payload_selection,
                post_readiness_field_errors,
            ));
        }

        // Supplying both originalSource and previewImageSource is rejected with
        // two INVALID userErrors, but only for ready files: Shopify resolves the
        // NON_READY_STATE gate above first (see media-file-update-validation-ordering).
        let ready_source_errors = inputs
            .iter()
            .enumerate()
            .flat_map(|(index, input)| validate_file_update_ready_source_fields(input, index))
            .collect::<Vec<_>>();
        if !ready_source_errors.is_empty() {
            return MutationOutcome::response(media_file_update_error_response(
                &response_key,
                &payload_selection,
                ready_source_errors,
            ));
        }

        let target_errors = inputs
            .iter()
            .enumerate()
            .flat_map(|(index, input)| self.validate_file_update_target(input, index))
            .collect::<Vec<_>>();
        if !target_errors.is_empty() {
            return MutationOutcome::response(media_file_update_error_response(
                &response_key,
                &payload_selection,
                target_errors,
            ));
        }

        let source_version_errors = inputs
            .iter()
            .enumerate()
            .filter_map(|(index, input)| file_update_source_version_conflict(input, index))
            .collect::<Vec<_>>();
        if !source_version_errors.is_empty() {
            return MutationOutcome::response(media_file_update_error_response(
                &response_key,
                &payload_selection,
                source_version_errors,
            ));
        }

        let reference_target_errors = self.validate_file_update_reference_targets(&inputs);
        if !reference_target_errors.is_empty() {
            return MutationOutcome::response(media_file_update_error_response(
                &response_key,
                &payload_selection,
                reference_target_errors,
            ));
        }

        let mut updated_files = Vec::new();
        for input in &inputs {
            let Some(id) = resolved_string_field(input, "id") else {
                continue;
            };
            let Some(mut file) = self.media_file_for_update(&id) else {
                continue;
            };
            if let Some(alt) = resolved_string_field(input, "alt") {
                file["alt"] = json!(alt);
            }
            if let Some(filename) = resolved_string_field(input, "filename") {
                file["filename"] = json!(filename);
                file["displayName"] = json!(filename);
            }
            // Source/preview updates invalidate the rendered image until the
            // backend reprocesses it. The immediate payload nulls `image` (Gleam
            // update_file_record) while the existing `preview`/`url` are retained,
            // because regeneration is asynchronous.
            let content_type = file
                .get("contentType")
                .and_then(Value::as_str)
                .map(str::to_string);
            let original_source =
                resolved_string_field(input, "originalSource").filter(|value| !value.is_empty());
            let preview_source = resolved_string_field(input, "previewImageSource")
                .filter(|value| !value.is_empty());
            let source_as_preview = if content_type.as_deref() == Some("IMAGE") {
                original_source.clone()
            } else {
                None
            };
            let explicit_preview = preview_source.or(source_as_preview);
            if explicit_preview.is_some() {
                file["image"] = Value::Null;
            }
            // GenericFile renders `url` from the accepted originalSource (Gleam
            // next_original_source for FILE). Image-type files defer to async
            // regeneration and keep their hydrated preview/url instead.
            if content_type.as_deref() == Some("FILE") {
                if let Some(source) = &original_source {
                    file["url"] = json!(source);
                }
            }
            file["updatedAt"] = json!("2024-01-01T00:00:59.000Z");
            self.store
                .staged
                .media_files
                .insert(id.clone(), file.clone());
            // Cascade: detaching a file from a product (referencesToRemove)
            // removes that file from the product's media and from every variant
            // that had it attached (Gleam parity: remove_media_ids_from_variants_for_products).
            let remove_products = list_string_field(input, "referencesToRemove");
            if !remove_products.is_empty() {
                self.store
                    .clear_media_ids(std::slice::from_ref(&id), Some(&remove_products));
            }
            updated_files.push(file);
        }
        let staged_ids = updated_files
            .iter()
            .filter_map(|file| file.get("id").and_then(Value::as_str).map(str::to_string))
            .collect::<Vec<_>>();
        let payload = json!({"files": updated_files, "userErrors": []});
        MutationOutcome::staged(
            ok_json(json!({"data": {response_key: selected_json(&payload, &payload_selection)}})),
            LogDraft::staged("fileUpdate", "media", staged_ids),
        )
    }

    pub(in crate::proxy) fn media_file_delete(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
        let (response_key, payload_selection) =
            primary_root_response_selection(query, variables, || "fileDelete".to_string());
        let ids = media_string_list_arg(query, variables, "fileIds")
            .into_iter()
            .map(|id| self.resolve_media_file_delete_id(&id))
            .collect::<Vec<_>>();
        // Hydrate the referenced files (and their owning products/variants) so
        // existence checks run against the real backend and the post-delete
        // cascade can clear the file from those owners.
        self.hydrate_media_file_references(request, &ids);
        let mut missing_ids = Vec::new();
        extend_unique_strings(
            &mut missing_ids,
            ids.iter()
                .filter(|id| !self.media_file_delete_target_exists(id)),
        );
        if !missing_ids.is_empty() {
            let payload = json!({
                "deletedFileIds": Value::Null,
                "userErrors": [file_delete_missing_ids_error(&missing_ids)]
            });
            return MutationOutcome::response(ok_json(json!({
                "data": {response_key: selected_json(&payload, &payload_selection)}
            })));
        }
        for id in &ids {
            self.store.staged.media_files.tombstone_staged(id);
        }
        // Cascade: detach the deleted files from every product/variant that
        // referenced them, so subsequent product.media / variant.media reads no
        // longer surface the removed file (Gleam parity: delete_staged_files).
        self.store.clear_media_ids(&ids, None);
        let payload = json!({"deletedFileIds": ids, "userErrors": []});
        MutationOutcome::staged(
            ok_json(json!({"data": {response_key: selected_json(&payload, &payload_selection)}})),
            LogDraft::staged("fileDelete", "media", ids),
        )
    }

    pub(in crate::proxy) fn media_file_acknowledge_update_failed(
        &self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
        let (response_key, payload_selection) =
            primary_root_response_selection(query, variables, || {
                "fileAcknowledgeUpdateFailed".to_string()
            });
        let file_ids = media_string_list_arg(query, variables, "fileIds");
        let mut missing_ids = Vec::new();
        extend_unique_strings(
            &mut missing_ids,
            file_ids
                .iter()
                .filter(|id| self.media_file_for_update(id).is_none()),
        );
        if !missing_ids.is_empty() {
            let payload = json!({
                "files": Value::Null,
                "userErrors": [file_ack_missing_ids_error(&missing_ids)]
            });
            return MutationOutcome::response(ok_json(json!({
                "data": {response_key: selected_json(&payload, &payload_selection)}
            })));
        }

        let mut non_ready_ids = Vec::new();
        extend_unique_strings(
            &mut non_ready_ids,
            file_ids.iter().filter(|id| {
                self.media_file_for_update(id)
                    .and_then(|file| {
                        file.get("fileStatus")
                            .and_then(Value::as_str)
                            .map(str::to_string)
                    })
                    .is_none_or(|status| status != "READY")
            }),
        );
        if !non_ready_ids.is_empty() {
            let payload = json!({
                "files": Value::Null,
                "userErrors": [file_ack_non_ready_error(&non_ready_ids)]
            });
            return MutationOutcome::response(ok_json(json!({
                "data": {response_key: selected_json(&payload, &payload_selection)}
            })));
        }

        let files = file_ids
            .iter()
            .filter_map(|id| self.media_file_for_update(id))
            .collect::<Vec<_>>();
        let payload = json!({"files": files, "userErrors": []});
        MutationOutcome::staged(
            ok_json(json!({"data": {response_key: selected_json(&payload, &payload_selection)}})),
            LogDraft::staged("fileAcknowledgeUpdateFailed", "media", file_ids),
        )
    }

    pub(in crate::proxy) fn media_staged_uploads_create(
        &mut self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> MutationOutcome {
        let (response_key, payload_selection) =
            primary_root_response_selection(query, variables, || "stagedUploadsCreate".to_string());
        let user_error_selection =
            selected_child_selection(&payload_selection, "userErrors").unwrap_or_default();
        if user_error_selection
            .iter()
            .any(|field| field.name == "code")
        {
            let operation_path = parsed_document(query, variables)
                .map(|document| document.operation_path)
                .unwrap_or_else(|| "mutation".to_string());
            return MutationOutcome::response(ok_json(json!({
                "errors": [{
                    "message": "Field 'code' doesn't exist on type 'UserError'",
                    "locations": [{"line": 7, "column": 9}],
                    "path": [operation_path, "stagedUploadsCreate", "userErrors", "code"],
                    "extensions": {"code": "undefinedField", "typeName": "UserError", "fieldName": "code"}
                }]
            })));
        }
        let inputs = media_object_list_arg(query, variables, "input");
        if let Some((index, resource)) = inputs
            .iter()
            .enumerate()
            .filter_map(|(index, input)| {
                resolved_string_field(input, "resource").map(|resource| (index, resource))
            })
            .find(|(_, resource)| !valid_staged_upload_resource(resource))
        {
            return MutationOutcome::response(ok_json(json!({
                "errors": [{
                    "message": format!("Variable $input of type [StagedUploadInput!]! was provided invalid value for {index}.resource (Expected \"{resource}\" to be one of: COLLECTION_IMAGE, FILE, IMAGE, MODEL_3D, PRODUCT_IMAGE, SHOP_IMAGE, VIDEO, BULK_MUTATION_VARIABLES, RETURN_LABEL, URL_REDIRECT_IMPORT, DISPUTE_FILE_UPLOAD)"),
                    "locations": [{"line": 2, "column": 35}],
                    "extensions": {
                        "code": "INVALID_VARIABLE",
                        "value": resolved_variables_json(variables).get("input").cloned().unwrap_or(Value::Null),
                        "problems": [{
                            "path": [index, "resource"],
                            "explanation": format!("Expected \"{resource}\" to be one of: COLLECTION_IMAGE, FILE, IMAGE, MODEL_3D, PRODUCT_IMAGE, SHOP_IMAGE, VIDEO, BULK_MUTATION_VARIABLES, RETURN_LABEL, URL_REDIRECT_IMPORT, DISPUTE_FILE_UPLOAD")
                        }]
                    }
                }]
            })));
        }
        // Validate every input up front so we know whether the mutation will
        // succeed. A successful mutation reserves a synthetic id for its log
        // entry before allocating target ids (Gleam reserves a MutationLogEntry
        // id first), keeping target ids in lockstep with parity.
        let validations: Vec<Vec<Value>> = inputs
            .iter()
            .enumerate()
            .map(|(index, input)| validate_staged_upload_input(input, index))
            .collect();
        if validations.iter().all(Vec::is_empty) {
            self.reserve_synthetic_log_id();
        }
        let mut errors = Vec::new();
        let mut targets = Vec::new();
        for ((index, input), input_errors) in inputs.iter().enumerate().zip(validations) {
            if input_errors.is_empty() {
                let id = self.next_synthetic_gid(&format!("StagedUploadTarget{index}"));
                targets.push(staged_upload_target(input, index, &id));
            } else {
                errors.extend(input_errors);
                targets.push(
                    json!({"url": Value::Null, "resourceUrl": Value::Null, "parameters": []}),
                );
            }
        }
        let payload = json!({"stagedTargets": targets, "userErrors": errors});
        let response = ok_json(json!({
            "data": {response_key: selected_json(&payload, &payload_selection)}
        }));
        if payload["userErrors"].as_array().is_some_and(Vec::is_empty) {
            self.record_bulk_operation_staged_uploads(&inputs, &targets);
            MutationOutcome::staged(
                response,
                LogDraft::staged("stagedUploadsCreate", "media", Vec::new()),
            )
        } else {
            MutationOutcome::response(response)
        }
    }

    fn record_bulk_operation_staged_uploads(
        &mut self,
        inputs: &[BTreeMap<String, ResolvedValue>],
        targets: &[Value],
    ) {
        for (input, target) in inputs.iter().zip(targets.iter()) {
            if resolved_string_field(input, "resource").as_deref()
                != Some("BULK_MUTATION_VARIABLES")
            {
                continue;
            }
            let file_size = resolved_u64_field(input, "fileSize");
            for path in staged_upload_target_paths(target) {
                self.store
                    .staged
                    .bulk_operation_staged_uploads
                    .insert(path, file_size);
            }
        }
    }

    pub(in crate::proxy) fn resolve_media_file_delete_id(&self, id: &str) -> String {
        if self.store.staged.media_files.contains_key(id) || !id.starts_with("gid://shopify/Video/")
        {
            return id.to_string();
        }
        let numeric_id = id.trim_start_matches("gid://shopify/Video/");
        let media_image_id = shopify_gid("MediaImage", numeric_id);
        if self.store.staged.media_files.contains_key(&media_image_id) {
            media_image_id
        } else {
            id.to_string()
        }
    }

    fn media_file_delete_target_exists(&self, id: &str) -> bool {
        self.store.staged.media_files.contains_key(id)
    }

    fn media_file_for_update(&self, id: &str) -> Option<Value> {
        let file = self
            .store
            .staged
            .media_files
            .get(id)
            .cloned()
            .or_else(|| seeded_media_file_for_update(id))?;
        let supplied_type = shopify_gid_resource_type(id);
        let actual_type = file.get("__typename").and_then(Value::as_str);
        if supplied_type.is_some() && actual_type.is_some() && supplied_type != actual_type {
            return None;
        }
        Some(file)
    }

    // Hydrate file-update target records from upstream when they are not already
    // known locally, so existence/type/state validation matches the real backend.
    // In replay these reads match the recorded cassette calls; against a live
    // backend they are ordinary `nodes(ids:)` reads. A node the backend does not
    // know about comes back null and is simply not staged (-> FILE_DOES_NOT_EXIST).
    fn hydrate_file_update_targets(
        &mut self,
        request: &Request,
        inputs: &[BTreeMap<String, ResolvedValue>],
    ) {
        if self.config.read_mode != ReadMode::LiveHybrid {
            return;
        }
        let mut missing_ids = Vec::new();
        extend_unique_strings(
            &mut missing_ids,
            inputs
                .iter()
                .filter_map(|input| resolved_string_field(input, "id"))
                .filter(|id| !id.is_empty() && self.media_file_for_update(id).is_none()),
        );
        if missing_ids.is_empty() {
            return;
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": MEDIA_FILE_UPDATE_HYDRATE_QUERY,
                "operationName": "MediaFileUpdateHydrate",
                "variables": { "fileIds": missing_ids },
            }),
        );
        if response.status >= 400 {
            return;
        }
        if let Some(nodes) = response.body["data"]["nodes"].as_array() {
            for node in nodes {
                if let Some(record) = media_file_record_from_node(node) {
                    if let Some(id) = record.get("id").and_then(Value::as_str).map(str::to_string) {
                        self.store.staged.media_files.insert(id, record);
                    }
                }
            }
        }
    }

    // Hydrate products referenced by referencesToAdd/referencesToRemove so that
    // attaching an existing product stays local-only after the read, and a missing
    // product surfaces REFERENCE_TARGET_DOES_NOT_EXIST.
    fn hydrate_referenced_products(
        &mut self,
        request: &Request,
        inputs: &[BTreeMap<String, ResolvedValue>],
    ) {
        if self.config.read_mode != ReadMode::LiveHybrid {
            return;
        }
        let mut product_ids = Vec::new();
        for input in inputs {
            for field in ["referencesToAdd", "referencesToRemove"] {
                extend_unique_strings(&mut product_ids, list_string_field(input, field));
            }
        }
        for product_id in product_ids {
            if product_id.is_empty() || self.store.product_by_id(&product_id).is_some() {
                continue;
            }
            let response = self.upstream_post(
                request,
                json!({
                    "query": MEDIA_PRODUCT_HYDRATE_QUERY,
                    "operationName": "MediaProductHydrate",
                    "variables": { "id": product_id },
                }),
            );
            if response.status >= 400 {
                continue;
            }
            if response.body["data"]["product"].is_object() {
                let product_node = response.body["data"]["product"].clone();
                self.observe_media_product_node(&product_node);
            }
        }
    }

    // Hydrate the products and variants that reference the given media files,
    // along with the file records themselves, from upstream. Used by fileDelete
    // (and the cascade that follows it) so existence checks and downstream
    // product.media / variant.media reads run against the real owner state.
    fn hydrate_media_file_references(&mut self, request: &Request, file_ids: &[String]) {
        if self.config.read_mode != ReadMode::LiveHybrid {
            return;
        }
        let mut missing = Vec::new();
        extend_unique_strings(
            &mut missing,
            file_ids.iter().filter(|id| {
                !id.is_empty() && !self.store.staged.media_files.contains_key(id.as_str())
            }),
        );
        if missing.is_empty() {
            return;
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": MEDIA_FILE_REFERENCES_HYDRATE_QUERY,
                "operationName": "MediaFileReferencesHydrate",
                "variables": { "fileIds": missing },
            }),
        );
        if response.status >= 400 {
            return;
        }
        let Some(nodes) = response.body["data"]["nodes"].as_array().cloned() else {
            return;
        };
        for node in nodes {
            // Stage the file record itself so the existence check passes.
            if let Some(record) = media_file_record_from_node(&node) {
                if let Some(id) = record.get("id").and_then(Value::as_str).map(str::to_string) {
                    if !self.store.staged.media_files.is_tombstoned(&id) {
                        self.store.staged.media_files.entry(id).or_insert(record);
                    }
                }
            }
            // Stage every product (and its variants/media) that references it.
            if let Some(references) = node.pointer("/references/nodes").and_then(Value::as_array) {
                for product_node in references.clone() {
                    self.observe_media_product_node(&product_node);
                }
            }
        }
    }

    // Stage a product node observed from a media hydration read: the product
    // record (raw media + variant nodes), a file record for each product media
    // node, and a `ProductVariantRecord` (with `media_ids`) for each variant.
    // Without the latter two, the cascade clear and downstream variant.media
    // reads would have nothing concrete to operate on.
    pub(in crate::proxy) fn observe_media_product_node(&mut self, product_node: &Value) {
        let Some(product_id) = product_node
            .get("id")
            .and_then(Value::as_str)
            .filter(|id| id.starts_with("gid://shopify/Product/"))
            .map(str::to_string)
        else {
            return;
        };
        self.store.stage_observed_product_json(product_node);
        for media_node in product_node
            .pointer("/media/nodes")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if let Some(record) = media_file_record_from_node(media_node) {
                if let Some(id) = record.get("id").and_then(Value::as_str).map(str::to_string) {
                    if !self.store.staged.media_files.is_tombstoned(&id) {
                        self.store.staged.media_files.entry(id).or_insert(record);
                    }
                }
            }
        }
        for variant_node in product_node
            .pointer("/variants/nodes")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let mut variant_value = variant_node.clone();
            if let Some(object) = variant_value.as_object_mut() {
                object
                    .entry("productId".to_string())
                    .or_insert_with(|| json!(product_id));
            }
            if let Some(variant) = product_variant_state_from_observed_json(&variant_value) {
                self.store.stage_product_variant(variant);
            }
        }
    }

    // Files referencing products that do not exist (after hydration) fail with
    // REFERENCE_TARGET_DOES_NOT_EXIST (Gleam parity: validate_file_update_reference_targets).
    fn validate_file_update_reference_targets(
        &self,
        inputs: &[BTreeMap<String, ResolvedValue>],
    ) -> Vec<Value> {
        let any_missing = inputs.iter().any(|input| {
            let mut product_ids = Vec::new();
            for field in ["referencesToAdd", "referencesToRemove"] {
                extend_unique_strings(&mut product_ids, list_string_field(input, field));
            }
            product_ids.iter().any(|product_id| {
                !product_id.is_empty() && self.store.product_by_id(product_id).is_none()
            })
        });
        if any_missing {
            vec![json!({
                "field": ["files"],
                "message": "The reference target does not exist",
                "code": "REFERENCE_TARGET_DOES_NOT_EXIST"
            })]
        } else {
            Vec::new()
        }
    }

    fn validate_file_update_target(
        &self,
        input: &BTreeMap<String, ResolvedValue>,
        index: usize,
    ) -> Vec<Value> {
        let Some(id) = resolved_string_field(input, "id") else {
            return Vec::new();
        };
        let Some(file) = self.media_file_for_update(&id) else {
            return Vec::new();
        };
        let typename = file
            .get("__typename")
            .and_then(Value::as_str)
            .unwrap_or("File");
        let allows_source_or_filename = matches!(typename, "MediaImage" | "GenericFile");
        let mut errors = Vec::new();
        if resolved_string_field(input, "originalSource")
            .filter(|value| !value.is_empty())
            .is_some()
            && !allows_source_or_filename
        {
            errors.push(json!({
                "field": ["files", index.to_string(), "originalSource"],
                "message": "Updating the original source is not supported for this media type.",
                "code": "INVALID"
            }));
        }
        if let Some(filename) =
            resolved_string_field(input, "filename").filter(|value| !value.is_empty())
        {
            if !allows_source_or_filename {
                errors.push(json!({
                    "field": ["files"],
                    "message": "Updating the filename is only supported on images and generic files",
                    "code": "UNSUPPORTED_MEDIA_TYPE_FOR_FILENAME_UPDATE"
                }));
            } else if let Some(existing) = file.get("filename").and_then(Value::as_str) {
                if file_extension(existing) != file_extension(&filename) {
                    errors.push(json!({
                        "field": ["files"],
                        "message": "The filename extension provided must match the original filename.",
                        "code": "INVALID_FILENAME_EXTENSION"
                    }));
                }
            }
        }
        errors
    }

    pub(in crate::proxy) fn media_files_read(
        &self,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
    ) -> Response {
        let mut data = serde_json::Map::new();
        for field in root_fields(query, variables).unwrap_or_default() {
            match field.name.as_str() {
                "files" => {
                    let mut files = self
                        .store
                        .staged
                        .media_files
                        .iter()
                        .filter(|(id, _)| !self.store.staged.media_files.is_tombstoned(id))
                        .map(|(_, file)| file.clone())
                        .collect::<Vec<_>>();
                    // Order by sortKey: ID (the numeric resource id), then honor
                    // `reverse`. Synthetic creation order tracks the numeric id,
                    // so this also approximates the default CREATED_AT ordering. A
                    // lexicographic string sort over the full gid would interleave
                    // by typename (GenericFile < MediaImage < Video), so it must be
                    // numeric.
                    files.sort_by_key(media_file_numeric_id);
                    if matches!(
                        field.arguments.get("reverse"),
                        Some(ResolvedValue::Bool(true))
                    ) {
                        files.reverse();
                    }
                    data.insert(
                        field.response_key,
                        selected_connection_json_with_args(
                            files,
                            &field.arguments,
                            &field.selection,
                            media_file_cursor,
                        ),
                    );
                }
                // Saved searches are not modeled yet, so the connection mirrors
                // Shopify's empty-state shape (no nodes, null cursors) rather than
                // being dropped from a combined `files`/`fileSavedSearches` read.
                "fileSavedSearches" => {
                    data.insert(
                        field.response_key,
                        selected_connection_json_with_args(
                            Vec::<Value>::new(),
                            &field.arguments,
                            &field.selection,
                            media_file_cursor,
                        ),
                    );
                }
                _ => continue,
            }
        }
        ok_json(json!({"data": Value::Object(data)}))
    }
}

fn media_object_list_arg(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
    key: &str,
) -> Vec<BTreeMap<String, ResolvedValue>> {
    let arguments = root_field_arguments(query, variables).unwrap_or_default();
    list_object_field(&arguments, key)
}

fn media_string_list_arg(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
    key: &str,
) -> Vec<String> {
    let arguments = root_field_arguments(query, variables).unwrap_or_default();
    list_string_field(&arguments, key)
}

fn media_invalid_field_arguments_response(
    response_key: &str,
    root_field: &str,
    message: &str,
) -> Response {
    ok_json(json!({
        "errors": [{
            "message": message,
            "locations": [{"line": 3, "column": 5}, {"line": 2, "column": 43}],
            "extensions": {"code": "INVALID_FIELD_ARGUMENTS"},
            "path": [root_field]
        }],
        "data": {response_key: Value::Null}
    }))
}

fn media_access_denied_response(response_key: &str, root_field: &str) -> Response {
    ok_json(json!({
        "errors": [{
            "message": "Access denied: Missing permission to manage products.",
            "locations": [{"line": 2, "column": 3}],
            "extensions": {
                "code": "ACCESS_DENIED",
                "documentation": "https://shopify.dev/api/usage/access-scopes"
            },
            "path": [root_field]
        }],
        "data": {response_key: Value::Null}
    }))
}

fn manage_products_denied(request: &Request) -> bool {
    request
        .headers
        .get("x-shopify-draft-proxy-manage-products")
        .map(|value| matches!(value.as_str(), "false" | "0" | "no"))
        .unwrap_or(false)
}

fn media_inputs_have_references(inputs: &[BTreeMap<String, ResolvedValue>]) -> bool {
    inputs.iter().any(|input| {
        !list_string_field(input, "referencesToAdd").is_empty()
            || !list_string_field(input, "referencesToRemove").is_empty()
    })
}

fn media_quota_errors(request: &Request, inputs: &[BTreeMap<String, ResolvedValue>]) -> Vec<Value> {
    let quota_header = request
        .headers
        .get("x-shopify-draft-proxy-media-quota-errors")
        .cloned()
        .unwrap_or_default();
    if quota_header.is_empty() {
        return Vec::new();
    }
    let requested = quota_header
        .split(',')
        .map(str::trim)
        .collect::<BTreeSet<_>>();
    inputs
        .iter()
        .enumerate()
        .filter_map(|(index, input)| {
            let content_type =
                resolved_string_field(input, "contentType").unwrap_or_else(|| "IMAGE".to_string());
            let code = if content_type == "VIDEO" && requested.contains("VIDEO_THROTTLE_EXCEEDED") {
                Some("VIDEO_THROTTLE_EXCEEDED")
            } else if content_type == "MODEL_3D" && requested.contains("MODEL3D_THROTTLE_EXCEEDED")
            {
                Some("MODEL3D_THROTTLE_EXCEEDED")
            } else if content_type != "IMAGE"
                && requested.contains("NON_IMAGE_MEDIA_PER_SHOP_LIMIT_EXCEEDED")
            {
                Some("NON_IMAGE_MEDIA_PER_SHOP_LIMIT_EXCEEDED")
            } else {
                None
            }?;
            Some(json!({
                "field": ["files", index.to_string(), "contentType"],
                "message": media_quota_message(code),
                "code": code
            }))
        })
        .collect()
}

fn media_quota_message(code: &str) -> &'static str {
    match code {
        "VIDEO_THROTTLE_EXCEEDED" => "Video upload throttle exceeded.",
        "MODEL3D_THROTTLE_EXCEEDED" => "Model 3D upload throttle exceeded.",
        "NON_IMAGE_MEDIA_PER_SHOP_LIMIT_EXCEEDED" => "Non-image media per shop limit exceeded.",
        _ => "Media quota exceeded.",
    }
}

fn validate_file_create_input(
    input: &BTreeMap<String, ResolvedValue>,
    index: usize,
) -> Option<Value> {
    let original_source = resolved_string_field(input, "originalSource").unwrap_or_default();
    if !is_http_url(&original_source) {
        return Some(json!({
            "field": ["files", index.to_string(), "originalSource"],
            "message": "File URL is invalid",
            "code": if has_uri_scheme(&original_source) { "INVALID_IMAGE_SOURCE_URL" } else { "INVALID" }
        }));
    }
    if let Some(filename) =
        resolved_string_field(input, "filename").filter(|value| !value.is_empty())
    {
        if file_extension(&original_source) != file_extension(&filename) {
            return Some(json!({
                "field": ["files", index.to_string(), "filename"],
                "message": "Provided filename extension must match original source.",
                "code": "MISMATCHED_FILENAME_AND_ORIGINAL_SOURCE"
            }));
        }
    }
    match resolved_string_field(input, "duplicateResolutionMode").as_deref() {
        Some("REPLACE") | Some("RAISE_ERROR") => {
            let mode = resolved_string_field(input, "duplicateResolutionMode").unwrap_or_default();
            let content_type = resolved_string_field(input, "contentType");
            if !duplicate_mode_allowed(&mode, content_type.as_deref()) {
                return Some(json!({
                    "field": ["files", index.to_string(), "duplicateResolutionMode"],
                    "message": format!("Duplicate resolution mode '{mode}' is not supported for '{}' media type.", duplicate_media_type_name(content_type.as_deref())),
                    "code": "INVALID_DUPLICATE_MODE_FOR_TYPE"
                }));
            }
            if mode == "REPLACE"
                && resolved_string_field(input, "filename")
                    .filter(|value| !value.is_empty())
                    .is_none()
            {
                return Some(json!({
                    "field": ["files", index.to_string(), "filename"],
                    "message": "Missing filename argument when attempting to use REPLACE duplicate mode.",
                    "code": "MISSING_FILENAME_FOR_DUPLICATE_MODE_REPLACE"
                }));
            }
        }
        _ => {}
    }
    None
}

fn validate_file_update_required_fields(
    input: &BTreeMap<String, ResolvedValue>,
    index: usize,
) -> Option<Value> {
    if resolved_string_field(input, "id")
        .filter(|value| !value.is_empty())
        .is_none()
    {
        return Some(json!({
            "field": ["files", index.to_string(), "id"],
            "message": "File id is required",
            "code": "REQUIRED"
        }));
    }
    None
}

fn validate_file_update_post_readiness_fields(
    input: &BTreeMap<String, ResolvedValue>,
    index: usize,
) -> Vec<Value> {
    let mut errors = Vec::new();
    if let Some(alt) = resolved_string_field(input, "alt") {
        if alt.chars().count() > 512 {
            errors.push(json!({
                "field": ["files", index.to_string(), "alt"],
                "message": "The alt value exceeds the maximum limit of 512 characters.",
                "code": "ALT_VALUE_LIMIT_EXCEEDED"
            }));
        }
    }
    // Gleam parity (validate_optional_url): an invalid originalSource OR
    // previewImageSource is always reported against the previewImageSource field
    // with the INVALID_IMAGE_SOURCE_URL code, regardless of which field carried it.
    for source_field in ["originalSource", "previewImageSource"] {
        if let Some(source) = resolved_string_field(input, source_field) {
            if !source.is_empty() && !is_http_url(&source) {
                errors.push(json!({
                    "field": ["files", index.to_string(), "previewImageSource"],
                    "message": "Invalid image source url value provided",
                    "code": "INVALID_IMAGE_SOURCE_URL"
                }));
            }
        }
    }
    errors
}

fn validate_file_update_ready_source_fields(
    input: &BTreeMap<String, ResolvedValue>,
    index: usize,
) -> Vec<Value> {
    let mut errors = Vec::new();
    let original = resolved_string_field(input, "originalSource").filter(|value| !value.is_empty());
    let preview =
        resolved_string_field(input, "previewImageSource").filter(|value| !value.is_empty());
    if original.is_some() && preview.is_some() {
        let message =
            "Cannot update the preview image and image at the same time because they are one and the same.";
        errors.push(json!({
            "field": ["files", index.to_string(), "previewImageSource"],
            "message": message,
            "code": "INVALID"
        }));
        errors.push(json!({
            "field": ["files", index.to_string(), "originalSource"],
            "message": message,
            "code": "INVALID"
        }));
    }
    errors
}

fn file_update_source_version_conflict(
    input: &BTreeMap<String, ResolvedValue>,
    index: usize,
) -> Option<Value> {
    let original = resolved_string_field(input, "originalSource").filter(|value| !value.is_empty());
    let preview =
        resolved_string_field(input, "previewImageSource").filter(|value| !value.is_empty());
    if (original.is_some() || preview.is_some())
        && resolved_string_field(input, "revertToVersionId")
            .filter(|value| !value.is_empty())
            .is_some()
    {
        return Some(json!({
            "field": ["files", index.to_string()],
            "message": "Specify either a source or revertToVersionId, not both.",
            "code": "CANNOT_SPECIFY_SOURCE_AND_VERSION_ID"
        }));
    }
    None
}

fn media_file_update_error_response(
    response_key: &str,
    payload_selection: &[SelectedField],
    user_errors: Vec<Value>,
) -> Response {
    let user_errors = dedupe_media_user_errors(user_errors);
    let payload = json!({"files": [], "userErrors": user_errors});
    ok_json(json!({"data": {response_key: selected_json(&payload, payload_selection)}}))
}

fn file_update_missing_ids_error(file_ids: &[String]) -> Value {
    let quoted = format!(
        "[{}]",
        file_ids
            .iter()
            .map(|id| format!("\"{id}\""))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let message = if file_ids.len() == 1 {
        format!("File id {quoted} does not exist.")
    } else {
        format!("File ids {quoted} do not exist.")
    };
    user_error(["files"], &message, Some("FILE_DOES_NOT_EXIST"))
}

fn file_ack_missing_ids_error(file_ids: &[String]) -> Value {
    let message = if file_ids.len() == 1 {
        format!("File id {} does not exist.", file_ids[0])
    } else {
        format!("File ids {} do not exist.", file_ids.join(","))
    };
    user_error(["fileIds"], &message, Some("FILE_DOES_NOT_EXIST"))
}

fn file_delete_missing_ids_error(file_ids: &[String]) -> Value {
    let message = if file_ids.len() == 1 {
        format!("File id {} does not exist.", file_ids[0])
    } else {
        format!("File ids {} do not exist.", file_ids.join(","))
    };
    user_error(["fileIds"], &message, Some("FILE_DOES_NOT_EXIST"))
}

fn file_ack_non_ready_error(file_ids: &[String]) -> Value {
    let message = if file_ids.len() == 1 {
        format!("File with id {} is not in the READY state.", file_ids[0])
    } else {
        format!(
            "Files with ids {} are not in the READY state.",
            file_ids.join(", ")
        )
    };
    user_error(["fileIds"], &message, Some("NON_READY_STATE"))
}

fn validate_staged_upload_input(
    input: &BTreeMap<String, ResolvedValue>,
    index: usize,
) -> Vec<Value> {
    let resource = resolved_string_field(input, "resource").unwrap_or_default();
    let filename = resolved_string_field(input, "filename").unwrap_or_default();
    let mime_type = resolved_string_field(input, "mimeType").unwrap_or_default();
    let mut errors = Vec::new();
    if matches!(resource.as_str(), "VIDEO" | "MODEL_3D")
        && resolved_string_field(input, "fileSize").is_none()
        && !matches!(input.get("fileSize"), Some(ResolvedValue::Int(_)))
    {
        errors.push(json!({
            "field": ["input", index.to_string(), "fileSize"],
            "message": format!("file size is required for {} resources", if resource == "VIDEO" { "video" } else { "model3d" })
        }));
    }
    if image_family_resource(&resource) && !valid_image_mime_type(&mime_type) {
        errors.push(json!({
            "field": ["input", index.to_string(), "mimeType"],
            "message": format!("{filename}: ({mime_type}) is not a recognized format")
        }));
    }
    errors
}

/// Encode the path-unsafe characters of a staged-upload URL segment, mirroring
/// the Gleam `encode_upload_segment` (`:` -> `%3A`, `/` -> `%2F`).
fn encode_upload_segment(value: &str) -> String {
    value.replace(':', "%3A").replace('/', "%2F")
}

/// Build a single staged upload target. The synthetic `id`
/// (`gid://shopify/StagedUploadTarget{index}/{n}`) is allocated by the caller so
/// that target ids stay in lockstep with the shared synthetic counter, exactly
/// as Gleam's `make_staged_target` does. URLs and signature material are inert
/// `shopify-draft-proxy.local` placeholders: the proxy never allocates real
/// external storage, so every signed value is a deterministic placeholder rather
/// than a captured Shopify secret.
fn staged_upload_target(input: &BTreeMap<String, ResolvedValue>, index: usize, id: &str) -> Value {
    let resource = resolved_string_field(input, "resource").unwrap_or_else(|| "FILE".to_string());
    let filename =
        resolved_string_field(input, "filename").unwrap_or_else(|| format!("upload-{index}"));
    let mime_type = resolved_string_field(input, "mimeType")
        .unwrap_or_else(|| "application/octet-stream".to_string());
    let method = resolved_string_field(input, "httpMethod").unwrap_or_else(|| "PUT".to_string());

    let key = format!("shopify-draft-proxy/{id}/{filename}");
    let url = format!(
        "https://shopify-draft-proxy.local/staged-uploads/{}",
        encode_upload_segment(id)
    );
    let resource_url = format!("{url}/{}", encode_upload_segment(&filename));

    // VIDEO and MODEL_3D resolve to Google signed-policy uploads
    // (GoogleAccessId/key/policy/signature); every other resource resolves to a
    // GCS form (POST) or simple object (PUT) upload.
    if matches!(resource.as_str(), "VIDEO" | "MODEL_3D") {
        let parameters: Vec<Value> = ["GoogleAccessId", "key", "policy", "signature"]
            .into_iter()
            .map(|name| {
                let value = if name == "key" {
                    key.clone()
                } else {
                    format!("shopify-draft-proxy-placeholder-{name}")
                };
                json!({"name": name, "value": value})
            })
            .collect();
        return json!({"url": url, "resourceUrl": resource_url, "parameters": parameters});
    }

    if method == "POST" {
        let parameters: Vec<Value> = [
            "Content-Type",
            "success_action_status",
            "acl",
            "key",
            "x-goog-date",
            "x-goog-credential",
            "x-goog-algorithm",
            "x-goog-signature",
            "policy",
        ]
        .into_iter()
        .map(|name| {
            let value = match name {
                "Content-Type" => mime_type.clone(),
                "success_action_status" => "201".to_string(),
                "acl" => "private".to_string(),
                "key" => key.clone(),
                "x-goog-algorithm" => "GOOG4-RSA-SHA256".to_string(),
                other => format!("shopify-draft-proxy-placeholder-{other}"),
            };
            json!({"name": name, "value": value})
        })
        .collect();
        json!({"url": url, "resourceUrl": resource_url, "parameters": parameters})
    } else {
        json!({
            "url": url,
            "resourceUrl": resource_url,
            "parameters": [
                {"name": "content_type", "value": mime_type},
                {"name": "acl", "value": "private"}
            ]
        })
    }
}

fn staged_upload_target_paths(target: &Value) -> Vec<String> {
    let mut paths = Vec::new();
    if let Some(resource_url) = target.get("resourceUrl").and_then(Value::as_str) {
        paths.push(resource_url.to_string());
        if let Some((_, path)) = resource_url.split_once("://") {
            if let Some((_, object_path)) = path.split_once('/') {
                paths.push(object_path.to_string());
            }
        }
    }
    if let Some(key) = target
        .get("parameters")
        .and_then(Value::as_array)
        .and_then(|parameters| {
            parameters.iter().find_map(|parameter| {
                (parameter.get("name").and_then(Value::as_str) == Some("key"))
                    .then(|| parameter.get("value").and_then(Value::as_str))
                    .flatten()
            })
        })
    {
        paths.push(key.to_string());
    }
    paths.sort();
    paths.dedup();
    paths
}

fn resolved_u64_field(fields: &BTreeMap<String, ResolvedValue>, name: &str) -> Option<u64> {
    match fields.get(name) {
        Some(ResolvedValue::Int(value)) if *value >= 0 => Some(*value as u64),
        Some(ResolvedValue::String(value)) => value.parse().ok(),
        _ => None,
    }
}

fn media_file_record(
    id: &str,
    content_type: &str,
    filename: &str,
    alt: &str,
    original_source: &str,
    file_status: &str,
    timestamp: &str,
) -> Value {
    let typename = media_file_gid_type(content_type);
    let mime_type = mime_type_for_filename(filename, content_type);
    let mut file = json!({
        "__typename": typename,
        "id": id,
        "alt": alt,
        "contentType": content_type,
        "createdAt": timestamp,
        "updatedAt": timestamp,
        "fileStatus": file_status,
        "updateStatus": file_status,
        "filename": filename,
        "displayName": filename,
        "fileErrors": [],
        "fileWarnings": [],
        "mimeType": mime_type
    });
    match typename {
        "MediaImage" => {
            file["image"] = json!({"url": original_source, "width": null, "height": null});
            file["preview"] =
                json!({"image": {"url": original_source, "width": null, "height": null}});
            file["mediaErrors"] = json!([]);
            file["mediaWarnings"] = json!([]);
        }
        "GenericFile" => {
            file["url"] = json!(original_source);
        }
        "Video" => {
            file["mediaContentType"] = json!("VIDEO");
            file["status"] = json!(file_status);
            file["preview"] = json!({"image": Value::Null});
            file["mediaErrors"] = json!([]);
            file["mediaWarnings"] = json!([]);
        }
        _ => {
            file["preview"] = json!({"image": Value::Null});
            file["mediaErrors"] = json!([]);
            file["mediaWarnings"] = json!([]);
        }
    }
    file
}

fn file_create_timestamp_for_index(index: usize) -> String {
    let offset_seconds = index + 1;
    let hours = offset_seconds / 3600;
    let minutes = (offset_seconds / 60) % 60;
    let seconds = offset_seconds % 60;
    format!("2024-01-01T{hours:02}:{minutes:02}:{seconds:02}.000Z")
}

fn media_file_numeric_id(file: &Value) -> u64 {
    file.get("id")
        .and_then(Value::as_str)
        .map(resource_id_tail)
        .and_then(|tail| tail.parse::<u64>().ok())
        .unwrap_or(0)
}

fn seeded_media_file_for_update(id: &str) -> Option<Value> {
    match id {
        "gid://shopify/MediaImage/43688017887538" => Some(media_file_record(
            id,
            "IMAGE",
            "filename-aggregation-single-1778241113775.jpg",
            "Seed",
            "https://cdn.example.com/filename-aggregation-single-1778241113775.jpg",
            "READY",
            "2026-05-08T00:00:00.000Z",
        )),
        "gid://shopify/MediaImage/43688017920306" => Some(media_file_record(
            id,
            "IMAGE",
            "filename-aggregation-multi-two-1778241113775.jpg",
            "Seed",
            "https://cdn.example.com/filename-aggregation-multi-two-1778241113775.jpg",
            "READY",
            "2026-05-08T00:00:00.000Z",
        )),
        "gid://shopify/ExternalVideo/43688017953074" => Some(media_file_record(
            id,
            "EXTERNAL_VIDEO",
            "filename-aggregation-video-one-1778241113775.mp4",
            "Seed",
            "https://www.youtube.com/watch?v=111",
            "READY",
            "2026-05-08T00:00:00.000Z",
        )),
        "gid://shopify/ExternalVideo/43688017985842" => Some(media_file_record(
            id,
            "EXTERNAL_VIDEO",
            "filename-aggregation-video-two-1778241113775.mp4",
            "Seed",
            "https://www.youtube.com/watch?v=222",
            "READY",
            "2026-05-08T00:00:00.000Z",
        )),
        _ => None,
    }
}

// Build a staged media-file record from an upstream `nodes(ids:)` hydration node,
// preserving the observed image/preview/url so reads echo the real upstream shape.
pub(super) fn media_file_record_from_node(node: &Value) -> Option<Value> {
    let id = node.get("id").and_then(Value::as_str)?.to_string();
    let typename = node.get("__typename").and_then(Value::as_str)?;
    let content_type = match typename {
        "MediaImage" => "IMAGE",
        "Video" => "VIDEO",
        "ExternalVideo" => "EXTERNAL_VIDEO",
        "Model3d" => "MODEL_3D",
        "GenericFile" => "FILE",
        _ => return None,
    };
    let created_at = node
        .get("createdAt")
        .and_then(Value::as_str)
        .unwrap_or("2024-01-01T00:00:00.000Z")
        .to_string();
    let updated_at = node
        .get("updatedAt")
        .and_then(Value::as_str)
        .or_else(|| node.get("createdAt").and_then(Value::as_str))
        .unwrap_or("2024-01-01T00:00:00.000Z")
        .to_string();
    let file_status = node
        .get("fileStatus")
        .and_then(Value::as_str)
        .or_else(|| node.get("status").and_then(Value::as_str))
        .unwrap_or("READY")
        .to_string();
    let source_url = node
        .get("url")
        .and_then(Value::as_str)
        .or_else(|| node.pointer("/image/url").and_then(Value::as_str))
        .or_else(|| node.pointer("/preview/image/url").and_then(Value::as_str))
        .map(str::to_string);
    let filename = source_url.as_deref().map(filename_from_source);

    let mut record = node.clone();
    record["__typename"] = json!(typename);
    record["id"] = json!(id);
    record["contentType"] = json!(content_type);
    record["createdAt"] = json!(created_at);
    record["updatedAt"] = json!(updated_at);
    record["fileStatus"] = json!(file_status);
    record["updateStatus"] = json!(file_status);
    record["fileErrors"] = json!([]);
    record["fileWarnings"] = json!([]);
    if let Some(filename) = &filename {
        record["filename"] = json!(filename);
        record["displayName"] = json!(filename);
        record["mimeType"] = json!(mime_type_for_filename(filename, content_type));
    } else if record.get("mimeType").is_none() {
        record["mimeType"] = Value::Null;
    }
    if matches!(
        typename,
        "MediaImage" | "Video" | "ExternalVideo" | "Model3d"
    ) {
        record["mediaErrors"] = json!([]);
        record["mediaWarnings"] = json!([]);
    }
    Some(record)
}

// Files-connection cursors are the record gid prefixed with `cursor:` (Gleam
// serializer convention), distinct from the bare-id cursors other connections
// emit via value_id_cursor.
fn media_file_cursor(record: &Value) -> String {
    format!("cursor:{}", value_id_cursor(record))
}

fn media_file_gid_type(content_type: &str) -> &'static str {
    match content_type {
        "VIDEO" => "Video",
        "EXTERNAL_VIDEO" => "ExternalVideo",
        "MODEL_3D" => "Model3d",
        "FILE" => "GenericFile",
        _ => "MediaImage",
    }
}

fn duplicate_mode_allowed(mode: &str, content_type: Option<&str>) -> bool {
    matches!(
        (mode, content_type),
        ("REPLACE", Some("IMAGE")) | ("RAISE_ERROR", Some("IMAGE")) | ("RAISE_ERROR", Some("FILE"))
    )
}

fn duplicate_media_type_name(content_type: Option<&str>) -> &str {
    match content_type {
        Some("FILE") => "GENERIC_FILE",
        Some(value) => value,
        None => "MISSING",
    }
}

fn filename_from_source(source: &str) -> String {
    source
        .split('?')
        .next()
        .unwrap_or(source)
        .rsplit('/')
        .next()
        .filter(|value| !value.is_empty())
        .unwrap_or("file")
        .to_string()
}

fn file_extension(value: &str) -> String {
    // Mirror Gleam derive_filename + file_extension: first reduce to the last
    // non-empty path segment (after stripping query/fragment), then take the
    // substring after that segment's final dot. A URL like
    // `https://www.w3.org/.../dummy` must yield "" — not "org/.../dummy" — even
    // though the host contains dots.
    let path = value.split(['?', '#']).next().unwrap_or(value);
    let filename = path
        .rsplit('/')
        .find(|segment| !segment.is_empty())
        .unwrap_or("");
    match filename.rsplit_once('.') {
        Some((_, extension)) => extension.to_ascii_lowercase(),
        None => String::new(),
    }
}

// Shopify infers FileContentType from the source/filename extension when the
// caller omits `contentType`, but the auto-detector maps only image/video
// results to typed media. Model3d and ExternalVideo require explicit contentType.
fn infer_content_type_from_source(filename: &str) -> &'static str {
    match file_extension(filename).as_str() {
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "heic" | "heif" => "IMAGE",
        "mp4" | "mov" | "m4v" | "webm" => "VIDEO",
        _ => "FILE",
    }
}

fn mime_type_for_filename(filename: &str, content_type: &str) -> &'static str {
    // Extension-first derivation (Gleam media/serializers.gleam `derive_mime_type`):
    // the recognized extension wins regardless of contentType, and only an
    // unrecognized extension falls back to the contentType default.
    match file_extension(filename).as_str() {
        "gif" => "image/gif",
        "heic" => "image/heic",
        "heif" => "image/heif",
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "webp" => "image/webp",
        "m4v" => "video/x-m4v",
        "mov" => "video/quicktime",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "glb" => "model/gltf-binary",
        "gltf" => "model/gltf+json",
        "usdz" => "model/vnd.usdz+zip",
        "csv" => "text/csv",
        "json" => "application/json",
        "pdf" => "application/pdf",
        "txt" => "text/plain",
        "zip" => "application/zip",
        _ => match content_type {
            "IMAGE" => "image/jpeg",
            "VIDEO" => "video/mp4",
            "MODEL_3D" => "model/gltf-binary",
            _ => "application/octet-stream",
        },
    }
}

fn is_http_url(value: &str) -> bool {
    value.starts_with("https://") || value.starts_with("http://")
}

fn has_uri_scheme(value: &str) -> bool {
    value.split_once(':').is_some_and(|(scheme, _)| {
        !scheme.is_empty() && scheme.chars().all(|c| c.is_ascii_alphabetic())
    })
}

fn image_family_resource(resource: &str) -> bool {
    matches!(
        resource,
        "IMAGE" | "PRODUCT_IMAGE" | "COLLECTION_IMAGE" | "SHOP_IMAGE"
    )
}

fn valid_staged_upload_resource(resource: &str) -> bool {
    matches!(
        resource,
        "COLLECTION_IMAGE"
            | "FILE"
            | "IMAGE"
            | "MODEL_3D"
            | "PRODUCT_IMAGE"
            | "SHOP_IMAGE"
            | "VIDEO"
            | "BULK_MUTATION_VARIABLES"
            | "RETURN_LABEL"
            | "URL_REDIRECT_IMPORT"
            | "DISPUTE_FILE_UPLOAD"
    )
}

fn valid_image_mime_type(mime_type: &str) -> bool {
    matches!(
        mime_type,
        "image/png"
            | "image/jpeg"
            | "image/jpg"
            | "image/gif"
            | "image/webp"
            | "image/heic"
            | "image/heif"
    )
}

fn dedupe_media_user_errors(values: Vec<Value>) -> Vec<Value> {
    let mut seen = BTreeSet::new();
    values
        .into_iter()
        .filter(|value| seen.insert(value.to_string()))
        .collect()
}
