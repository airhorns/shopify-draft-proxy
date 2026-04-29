import type { ProxyRuntimeContext } from '../runtime-context.js';
import { getFieldArguments, getRootFields } from '../../graphql/root-field.js';
import { getDocumentFragments, getFieldResponseKey, readNullableStringArgument } from '../graphql-helpers.js';
import type { OrderSearchExtensionEntry } from './serializers.js';
import {
  buildDraftOrderInvalidSearchExtension,
  findDraftOrderTagById,
  listOrderFulfillmentOrders,
  listOrderFulfillments,
  listOrderReverseDeliveries,
  listOrderReverseFulfillmentOrders,
  listOrderReturns,
  prepareAssignedFulfillmentOrders,
  prepareTopLevelFulfillmentOrders,
  serializeAbandonedCheckoutsConnection,
  serializeAbandonedCheckoutsCount,
  serializeAbandonmentNode,
  serializeDraftOrderAvailableDeliveryOptions,
  serializeDraftOrderNode,
  serializeDraftOrderSavedSearchesConnection,
  serializeDraftOrdersConnection,
  serializeDraftOrdersCount,
  serializeDraftOrderTag,
  serializeOrderFulfillment,
  serializeOrderFulfillmentOrder,
  serializeOrderFulfillmentOrdersConnection,
  serializeOrderNode,
  serializeOrderReverseDelivery,
  serializeOrderReverseFulfillmentOrder,
  serializeOrderReturn,
  serializeOrdersConnection,
  serializeOrdersCount,
  sortFulfillmentOrdersForConnection,
} from './serializers.js';

export function handleOrderQuery(
  runtime: ProxyRuntimeContext,
  document: string,
  variables: Record<string, unknown> = {},
): { data: Record<string, unknown>; extensions?: { search: OrderSearchExtensionEntry[] } } {
  const data: Record<string, unknown> = {};
  const fragments = getDocumentFragments(document);
  const orders = runtime.store.getOrders();
  const abandonedCheckouts = runtime.store.getAbandonedCheckouts();
  const fulfillments = listOrderFulfillments(orders);
  const fulfillmentOrders = listOrderFulfillmentOrders(orders);
  const orderReturns = listOrderReturns(orders);
  const reverseFulfillmentOrders = listOrderReverseFulfillmentOrders(orders);
  const reverseDeliveries = listOrderReverseDeliveries(orders);
  const searchExtensions: OrderSearchExtensionEntry[] = [];

  for (const field of getRootFields(document)) {
    const key = getFieldResponseKey(field);

    switch (field.name.value) {
      case 'order': {
        const id = readNullableStringArgument(field, 'id', variables);
        const order = id ? runtime.store.getOrderById(id) : null;
        data[key] = order ? serializeOrderNode(runtime, field, order, variables) : null;
        break;
      }
      case 'orders':
        data[key] = serializeOrdersConnection(runtime, field, orders, variables);
        break;
      case 'ordersCount':
        data[key] = serializeOrdersCount(field, orders, variables);
        break;
      case 'abandonedCheckouts':
        data[key] = serializeAbandonedCheckoutsConnection(runtime, field, abandonedCheckouts, variables, fragments);
        break;
      case 'abandonedCheckoutsCount':
        data[key] = serializeAbandonedCheckoutsCount(runtime, field, abandonedCheckouts, variables);
        break;
      case 'abandonment': {
        const id = readNullableStringArgument(field, 'id', variables);
        const abandonment = id ? runtime.store.getAbandonmentById(id) : null;
        data[key] = abandonment ? serializeAbandonmentNode(runtime, field, abandonment, variables, fragments) : null;
        break;
      }
      case 'abandonmentByAbandonedCheckoutId': {
        const abandonedCheckoutId = readNullableStringArgument(field, 'abandonedCheckoutId', variables);
        const abandonment = abandonedCheckoutId
          ? runtime.store.getAbandonmentByAbandonedCheckoutId(abandonedCheckoutId)
          : null;
        data[key] = abandonment ? serializeAbandonmentNode(runtime, field, abandonment, variables, fragments) : null;
        break;
      }
      case 'fulfillment': {
        const id = readNullableStringArgument(field, 'id', variables);
        const fulfillment = id ? (fulfillments.find((candidate) => candidate.id === id) ?? null) : null;
        data[key] = fulfillment ? serializeOrderFulfillment(field, fulfillment, variables) : null;
        break;
      }
      case 'fulfillmentOrder': {
        const id = readNullableStringArgument(field, 'id', variables);
        const fulfillmentOrder = id ? (fulfillmentOrders.find((candidate) => candidate.id === id) ?? null) : null;
        data[key] = fulfillmentOrder ? serializeOrderFulfillmentOrder(field, fulfillmentOrder, variables) : null;
        break;
      }
      case 'return': {
        const id = readNullableStringArgument(field, 'id', variables);
        const match = id ? (orderReturns.find((candidate) => candidate.orderReturn.id === id) ?? null) : null;
        data[key] = match ? serializeOrderReturn(runtime, field, match.orderReturn, variables, match.order) : null;
        break;
      }
      case 'reverseFulfillmentOrder': {
        const id = readNullableStringArgument(field, 'id', variables);
        const match = id
          ? (reverseFulfillmentOrders.find((candidate) => candidate.reverseFulfillmentOrder.id === id) ?? null)
          : null;
        data[key] = match
          ? serializeOrderReverseFulfillmentOrder(
              runtime,
              field,
              match.reverseFulfillmentOrder,
              match.orderReturn,
              match.order,
              variables,
            )
          : null;
        break;
      }
      case 'reverseDelivery': {
        const id = readNullableStringArgument(field, 'id', variables);
        const match = id ? (reverseDeliveries.find((candidate) => candidate.reverseDelivery.id === id) ?? null) : null;
        data[key] = match
          ? serializeOrderReverseDelivery(
              runtime,
              field,
              match.reverseDelivery,
              match.reverseFulfillmentOrder,
              match.orderReturn,
              match.order,
              variables,
            )
          : null;
        break;
      }
      case 'fulfillmentOrders':
        data[key] = serializeOrderFulfillmentOrdersConnection(
          field,
          prepareTopLevelFulfillmentOrders(field, fulfillmentOrders, variables),
          variables,
          { includeCursors: true },
        );
        break;
      case 'assignedFulfillmentOrders':
        data[key] = serializeOrderFulfillmentOrdersConnection(
          field,
          prepareAssignedFulfillmentOrders(field, fulfillmentOrders, variables),
          variables,
          { includeCursors: true },
        );
        break;
      case 'manualHoldsFulfillmentOrders':
        data[key] = serializeOrderFulfillmentOrdersConnection(
          field,
          sortFulfillmentOrdersForConnection(
            fulfillmentOrders.filter((fulfillmentOrder) => (fulfillmentOrder.fulfillmentHolds ?? []).length > 0),
            field,
            variables,
          ),
          variables,
          { includeCursors: true },
        );
        break;
      case 'draftOrder': {
        const id = readNullableStringArgument(field, 'id', variables);
        const draftOrder = id ? runtime.store.getDraftOrderById(id) : null;
        data[key] = draftOrder ? serializeDraftOrderNode(runtime, field, draftOrder) : null;
        break;
      }
      case 'draftOrders': {
        const args = getFieldArguments(field, variables);
        data[key] = serializeDraftOrdersConnection(runtime, field, runtime.store.getDraftOrders(), variables);
        const searchExtension = buildDraftOrderInvalidSearchExtension(args['query'], [key]);
        if (searchExtension) {
          searchExtensions.push(searchExtension);
        }
        break;
      }
      case 'draftOrdersCount': {
        const args = getFieldArguments(field, variables);
        data[key] = serializeDraftOrdersCount(field, runtime.store.getDraftOrders(), variables);
        const searchExtension = buildDraftOrderInvalidSearchExtension(args['query'], [key]);
        if (searchExtension) {
          searchExtensions.push(searchExtension);
        }
        break;
      }
      case 'draftOrderAvailableDeliveryOptions':
        data[key] = serializeDraftOrderAvailableDeliveryOptions(field);
        break;
      case 'draftOrderSavedSearches':
        data[key] = serializeDraftOrderSavedSearchesConnection(field, variables);
        break;
      case 'draftOrderTag': {
        const id = readNullableStringArgument(field, 'id', variables);
        const tag = id ? findDraftOrderTagById(runtime, id) : null;
        data[key] = tag ? serializeDraftOrderTag(field, tag) : null;
        break;
      }
      default:
        break;
    }
  }

  if (searchExtensions.length > 0) {
    return {
      data,
      extensions: {
        search: searchExtensions,
      },
    };
  }

  return { data };
}
