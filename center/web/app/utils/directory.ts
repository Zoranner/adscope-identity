import type { GroupRecord, OrganizationalUnit, OuTreeItem, UserRecord } from '~/types/admin'

export function sortOus(ous: OrganizationalUnit[]): OrganizationalUnit[] {
  return [...ous].sort((left, right) => {
    const byName = left.name.localeCompare(right.name, 'zh-Hans-CN')
    return byName === 0 ? left.id.localeCompare(right.id) : byName
  })
}

export function flattenOus(
  ous: OrganizationalUnit[],
  users: UserRecord[],
  groups: GroupRecord[],
): OuTreeItem[] {
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
        userCount: users.filter((user) => user.organizational_unit_id === ou.id).length,
        groupCount: groups.filter((group) => group.organizational_unit_id === ou.id).length,
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
        userCount: users.filter((user) => user.organizational_unit_id === ou.id).length,
        groupCount: groups.filter((group) => group.organizational_unit_id === ou.id).length,
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
