/** macOS（含 iPad 桌面端 UA）判定。集中一处，避免每个组件各写一份正则。 */
export const isMacOSPlatform = /Mac|iPhone|iPad|iPod/.test(navigator.platform);
