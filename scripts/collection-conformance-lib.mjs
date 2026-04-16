export function pickCollectionCaptureSeed(payload) {
  const directEdges = payload?.data?.product?.collections?.edges;
  if (Array.isArray(directEdges)) {
    for (const edge of directEdges) {
      const node = edge?.node;
      if (typeof node?.id === 'string' && typeof node?.title === 'string' && typeof node?.handle === 'string') {
        return {
          id: node.id,
          title: node.title,
          handle: node.handle,
        };
      }
    }
  }

  const productEdges = payload?.data?.products?.edges;
  if (Array.isArray(productEdges)) {
    for (const productEdge of productEdges) {
      const collectionEdges = productEdge?.node?.collections?.edges;
      if (!Array.isArray(collectionEdges)) {
        continue;
      }
      for (const collectionEdge of collectionEdges) {
        const node = collectionEdge?.node;
        if (typeof node?.id === 'string' && typeof node?.title === 'string' && typeof node?.handle === 'string') {
          return {
            id: node.id,
            title: node.title,
            handle: node.handle,
          };
        }
      }
    }
  }

  throw new Error('Could not find a sample collection from ProductDetail capture');
}
