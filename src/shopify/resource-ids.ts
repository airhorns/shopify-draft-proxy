export function compareShopifyResourceIds(leftId: string, rightId: string): number {
  const leftTail = Number.parseInt(leftId.split('/').at(-1) ?? '', 10);
  const rightTail = Number.parseInt(rightId.split('/').at(-1) ?? '', 10);
  if (Number.isFinite(leftTail) && Number.isFinite(rightTail)) {
    return leftTail - rightTail;
  }

  return leftId.localeCompare(rightId);
}

export function compareNullableStrings(left: string | null | undefined, right: string | null | undefined): number {
  if (left === right) return 0;
  if (!left) return 1;
  if (!right) return -1;
  return left.localeCompare(right);
}
