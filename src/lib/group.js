export function groupConsecutive(items, keyFn) {
  const groups = [];
  for (const item of items) {
    const key = keyFn(item);
    const last = groups[groups.length - 1];
    if (key != null && last && last.key === key) {
      last.items.push(item);
    } else {
      groups.push({ key, items: [item] });
    }
  }
  return groups;
}
