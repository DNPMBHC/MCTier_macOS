import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');

/**
 * Rust 模块的候选目录。
 *
 * 平台化重构会把模块从 src-tauri/src/modules/ 逐步迁往 crates/mctier-core/src/
 * 以及后续的 crates/mctier-platform-<os>/ crate。测试不应该跟着一起搬家——那样每搬一个
 * 模块就要改一批测试路径，而且改动本身与被测行为无关。这里改为按**模块名**解析，
 * 迁移时只需在这里追加候选目录。
 */
const MODULE_DIRS = [
  'src-tauri/src/modules',
  'crates/mctier-core/src',
];

/** 按模块名（不带 .rs）解析 Rust 源文件真实路径。 */
export function rustModulePath(name) {
  for (const dir of MODULE_DIRS) {
    const candidate = path.join(root, dir, `${name}.rs`);
    if (fs.existsSync(candidate)) return candidate;
  }
  throw new Error(
    `找不到 Rust 模块 ${name}.rs；已搜索: ${MODULE_DIRS.join(', ')}。` +
      `若该模块已迁往新 crate，请把目录加进 tests/helpers/rustSource.mjs 的 MODULE_DIRS。`,
  );
}

/** 读取某个 Rust 模块的源码。 */
export function readRustModule(name) {
  return fs.readFileSync(rustModulePath(name), 'utf8');
}

/** 读取仓库内任意文件（相对仓库根）。 */
export function readRepoFile(...segments) {
  return fs.readFileSync(path.join(root, ...segments), 'utf8');
}

export { root as repoRoot };
