use crate::proxy::*;

mod delegate_access;
mod installation;
mod purchases;
mod subscriptions;

fn app_domain_confirmation_url_from_arguments(
    arguments: &BTreeMap<String, ResolvedValue>,
) -> String {
    resolved_string_field(arguments, "returnUrl")
        .filter(|value| !value.trim().is_empty())
        .map(|value| app_confirmation_url_with_marker(&value))
        .unwrap_or_else(|| {
            app_confirmation_url_with_marker("shopify-draft-proxy://local-confirmation")
        })
}

fn app_domain_confirmation_url_for_request(
    request: &Request,
    shopify_admin_origin: &str,
) -> String {
    let base = request_header(request, "x-shopify-draft-proxy-app-url")
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| shopify_admin_origin.to_string());
    let base = app_local_confirmation_base_url(&base);
    app_confirmation_url_with_marker(&base)
}

fn app_local_confirmation_base_url(base: &str) -> String {
    match url::Url::parse(base) {
        Ok(mut url) if matches!(url.path(), "" | "/") => {
            url.set_path("/local-confirmation");
            url.to_string()
        }
        _ => base.to_string(),
    }
}

fn app_confirmation_url_with_marker(base: &str) -> String {
    match url::Url::parse(base) {
        Ok(mut url) => {
            url.query_pairs_mut()
                .append_pair("shopify_draft_proxy_confirmation", "1");
            url.to_string()
        }
        Err(_) => {
            let separator = if base.contains('?') { '&' } else { '?' };
            format!("{base}{separator}shopify_draft_proxy_confirmation=1")
        }
    }
}
