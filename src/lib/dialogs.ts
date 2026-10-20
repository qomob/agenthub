import { open } from '@tauri-apps/plugin-dialog'
import { openPath, openUrl } from '@tauri-apps/plugin-opener'

import { ApiError, isDesktop } from './api'

function requireDesktop(): void {
  if (!isDesktop()) {
    throw new ApiError('文件选择器只在桌面应用中可用：请运行 pnpm tauri dev。', 'not_tauri')
  }
}

/** Native directory picker. Returns null when the user cancels. */
export async function chooseDirectory(defaultPath?: string): Promise<string | null> {
  requireDesktop()
  const selected = await open({
    directory: true,
    multiple: false,
    defaultPath: defaultPath && defaultPath.length > 0 ? defaultPath : undefined,
  })
  return typeof selected === 'string' ? selected : null
}

/** Native file picker. `extensions` are given without a leading dot. */
export async function chooseFile(
  extensions: string[],
  defaultPath?: string,
): Promise<string | null> {
  requireDesktop()
  const selected = await open({
    directory: false,
    multiple: false,
    defaultPath: defaultPath && defaultPath.length > 0 ? defaultPath : undefined,
    filters: extensions.length > 0 ? [{ name: 'Files', extensions }] : undefined,
  })
  return typeof selected === 'string' ? selected : null
}

export async function openExternal(url: string): Promise<void> {
  requireDesktop()
  await openUrl(url)
}

export async function revealPath(path: string): Promise<void> {
  requireDesktop()
  await openPath(path)
}
