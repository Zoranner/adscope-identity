import type { OrganizationalUnit, OuTreeItem } from '~/types/admin'

export function sortOus(ous: OrganizationalUnit[]): OrganizationalUnit[] {
  return [...ous].sort((left, right) => {
    const byName = left.name.localeCompare(right.name, 'zh-Hans-CN')
    return byName === 0 ? left.id.localeCompare(right.id) : byName
  })
}

export function flattenOus(ous: OrganizationalUnit[]): OuTreeItem[] {
  const byParent = new Map<string | null, OrganizationalUnit[]>()
  for (const ou of ous) {
    const siblings = byParent.get(ou.parent_id) ?? []
    siblings.push(ou)
    byParent.set(ou.parent_id, siblings)
  }

  for (const [parentId, siblings] of byParent.entries()) {
    byParent.set(parentId, sortOus(siblings))
  }

  const visited = new Set<string>()
  const items: OuTreeItem[] = []
  const pushChildren = (parentId: string | null, depth: number) => {
    for (const ou of byParent.get(parentId) ?? []) {
      if (visited.has(ou.id)) {
        continue
      }
      visited.add(ou.id)
      items.push({
        ou,
        depth,
      })
      pushChildren(ou.id, depth + 1)
    }
  }

  pushChildren(null, 0)
  for (const ou of sortOus(ous)) {
    if (!visited.has(ou.id)) {
      visited.add(ou.id)
      items.push({
        ou,
        depth: 0,
      })
      pushChildren(ou.id, 1)
    }
  }

  return items
}

export function ouName(
  ous: OrganizationalUnit[],
  ouId: string | null | undefined,
): string {
  if (!ouId) {
    return '-'
  }
  return ous.find((ou) => ou.id === ouId)?.name ?? ouId
}

export type OuTreeNode = {
  key: string
  title: string
  children?: OuTreeNode[]
}

export function buildOuTree(ous: OrganizationalUnit[]): OuTreeNode[] {
  const byParent = new Map<string | null, OrganizationalUnit[]>()
  for (const ou of ous) {
    const siblings = byParent.get(ou.parent_id) ?? []
    siblings.push(ou)
    byParent.set(ou.parent_id, siblings)
  }
  for (const [parentId, siblings] of byParent.entries()) {
    byParent.set(parentId, sortOus(siblings))
  }

  const visited = new Set<string>()
  const toNode = (ou: OrganizationalUnit): OuTreeNode => ({
    key: ou.id,
    title: ou.name,
  })
  const build = (parentId: string | null): OuTreeNode[] => {
    const nodes: OuTreeNode[] = []
    for (const ou of byParent.get(parentId) ?? []) {
      if (visited.has(ou.id)) {
        continue
      }
      visited.add(ou.id)
      const children = build(ou.id)
      const node = toNode(ou)
      if (children.length > 0) {
        node.children = children
      }
      nodes.push(node)
    }
    return nodes
  }

  const roots = build(null)
  const orphans = sortOus(ous).filter((ou) => !visited.has(ou.id))
  for (const ou of orphans) {
    visited.add(ou.id)
  }
  return [...roots, ...orphans.map(toNode)]
}
