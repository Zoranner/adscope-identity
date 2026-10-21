import { expect, test } from 'bun:test'
import { Glob } from 'bun'

const appCss = await Bun.file(new URL('../app/assets/css/main.css', import.meta.url)).text()
const adminShell = await Bun.file(
  new URL('../app/components/admin/AdminShell.vue', import.meta.url),
).text()
const appRoot = await Bun.file(new URL('../app/app.vue', import.meta.url)).text()

test('delegates theme and shared chrome to Stellar UI', () => {
  expect(appCss).not.toMatch(/color-scheme|#[0-9a-f]{3,8}\b/i)
  expect(appCss).toContain('@import "tailwindcss"')
  expect(adminShell).toContain('useTheme')
  expect(adminShell).toContain('<NavBar')
  expect(adminShell).toContain('<DropdownMenu')
  expect(appRoot).toContain('<NotificationContainer')
})

test('styling uses utility classes only: no <style> blocks in any component', async () => {
  const offenders: string[] = []
  for await (const file of new Glob('{app/**/*.vue}').scan('.')) {
    const source = await Bun.file(file).text()
    if (source.includes('<style')) {
      offenders.push(file)
    }
  }
  expect(offenders).toEqual([])
})
