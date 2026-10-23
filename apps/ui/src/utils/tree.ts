import type { NodeDetail, Rect, TreeNode } from "../engine/contract";

/**
 * The engine sends `summary.nodes` as a flat pre-order list with `depth` and
 * `parent`. The Layers panel needs a filtered view that respects collapsed
 * subtrees and a search filter (matched rows plus their ancestors).
 */
export function flattenVisibleNodes(
  nodes: readonly TreeNode[],
  collapsed: ReadonlySet<string>,
  filter = "",
): TreeNode[] {
  const query = filter.trim().toLowerCase();
  if (query === "") {
    const byId = new Map(nodes.map((node) => [node.id, node]));
    return nodes.filter((node) => !hasCollapsedAncestor(node, byId, collapsed));
  }

  // Search mode: show matching nodes and every ancestor needed to reach them.
  const visible = new Set<string>();
  const byId = new Map(nodes.map((node) => [node.id, node]));
  for (const node of nodes) {
    if (!node.name.toLowerCase().includes(query)) continue;
    visible.add(node.id);
    let parentId = node.parent;
    // Walk ancestors with a bound so malformed data cannot loop forever.
    for (let guard = 0; parentId && guard < 512; guard += 1) {
      visible.add(parentId);
      parentId = byId.get(parentId)?.parent ?? null;
    }
  }
  return nodes.filter((node) => visible.has(node.id));
}

function hasCollapsedAncestor(
  node: TreeNode,
  byId: ReadonlyMap<string, TreeNode>,
  collapsed: ReadonlySet<string>,
): boolean {
  let parentId = node.parent;
  for (let guard = 0; parentId && guard < 512; guard += 1) {
    if (collapsed.has(parentId)) return true;
    parentId = byId.get(parentId)?.parent ?? null;
  }
  return false;
}

export function findNode(nodes: readonly TreeNode[], id: string): TreeNode | null {
  return nodes.find((node) => node.id === id) ?? null;
}

export function nodeAncestors(nodes: readonly TreeNode[], id: string): TreeNode[] {
  const byId = new Map(nodes.map((node) => [node.id, node]));
  const ancestors: TreeNode[] = [];
  let current = byId.get(id)?.parent ?? null;
  for (let guard = 0; current && guard < 512; guard += 1) {
    const parent = byId.get(current);
    if (!parent) break;
    ancestors.push(parent);
    current = parent.parent;
  }
  return ancestors;
}

/** Union of world-space AABBs; null for an empty selection. */
export function unionBounds(details: readonly NodeDetail[]): Rect | null {
  if (details.length === 0) return null;
  let minX = Infinity;
  let minY = Infinity;
  let maxX = -Infinity;
  let maxY = -Infinity;
  for (const detail of details) {
    const b = detail.absolute_bounds;
    minX = Math.min(minX, b.x);
    minY = Math.min(minY, b.y);
    maxX = Math.max(maxX, b.x + b.w);
    maxY = Math.max(maxY, b.y + b.h);
  }
  if (!Number.isFinite(minX) || !Number.isFinite(minY)) return null;
  return { x: minX, y: minY, w: Math.max(0, maxX - minX), h: Math.max(0, maxY - minY) };
}

/** Depth-first search over the flat pre-order list; returns the node and its index. */
export function findNodeIndex(nodes: readonly TreeNode[], id: string): number {
  return nodes.findIndex((node) => node.id === id);
}

/** All ids in a subtree rooted at `rootId` (flat list order). */
export function subtreeIds(nodes: readonly TreeNode[], rootId: string): string[] {
  const root = findNode(nodes, rootId);
  if (!root) return [];
  const origin = nodes.findIndex((node) => node.id === rootId);
  if (origin === -1) return [];
  const ids = [rootId];
  for (let i = origin + 1; i < nodes.length; i += 1) {
    const node = nodes[i];
    if (!node || node.depth <= root.depth) break;
    ids.push(node.id);
  }
  return ids;
}
