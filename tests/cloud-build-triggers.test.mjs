import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const read = (relativePath) => fs.readFileSync(path.join(root, relativePath), 'utf8');

/**
 * 按行结构断言的文本读取。
 *
 * CI 的 frontend job 跑在 windows-latest 上，checkout 出来的 *.yml 是 CRLF。
 * 直接在正则里写 `\n` 会在那边全部失配（本地 macOS/Linux 是 LF，所以看不出来）。
 * 这里统一归一成 LF，断言只关心行结构，不关心行尾。
 */
const readLines = (relativePath) => read(relativePath).replace(/\r\n/g, '\n');

test('云端打包只在 tag 与手动触发时运行，普通 push 不出包', () => {
  const workflow = readLines('.github/workflows/build.yml');

  // 普通 push 到分支会跑满 5 个 job、约 20 分钟并占用 macOS runner 配额。
  // 出包改为按需触发：推 v* tag 自动，其余走本地脚本的 workflow_dispatch。
  const onBlock = /^on:\n([\s\S]*?)\npermissions:/m.exec(workflow);
  assert.ok(onBlock, 'build.yml 应该有 on: 块');
  assert.match(onBlock[1], /^ {2}push:\n {4}tags:/m, 'push 只按 tags 触发');
  assert.doesNotMatch(onBlock[1], /^ {4}branches:/m, 'push 不应再按分支触发');
  assert.match(onBlock[1], /^ {2}workflow_dispatch:/m, '必须保留手动触发');
  assert.doesNotMatch(onBlock[1], /^ {2}pull_request:/m, 'PR 由 ci.yml 负责');

  // tag 推送的构建不能被后续推送取消，否则会丢掉 Release 的产物。
  assert.match(workflow, /cancel-in-progress:\s*false/);
});

test('Release 门禁要求 macOS 两个架构都在，而不是「至少一个 dmg」', () => {
  const workflow = readLines('.github/workflows/build.yml');

  // Intel 腿挂了 continue-on-error。若只按「至少一个 dmg」放行，Apple Silicon
  // 被 runner 容量挤掉时 Release 仍会创建，但只剩 Intel 包 —— 而 aarch64 才是
  // 主目标。这里锁住「两个架构分别校验」这条性质。
  assert.match(workflow, /\*_aarch64\.dmg/, '应识别 aarch64 DMG 后缀');
  assert.match(workflow, /\*_x64\.dmg/, '应识别 x64 DMG 后缀');
  assert.match(workflow, /have_aarch64/, '应有 aarch64 命中标记');
  assert.match(workflow, /have_x86_64/, '应有 x64 命中标记');
  assert.match(
    workflow,
    /\(\(have_aarch64 && have_x86_64\)\)/,
    '两个架构必须同时命中才放行',
  );
  // tauri 的 DMG 命名是 <product>_<version>_<arch>.dmg，所以后缀判断才成立。
  // 一旦升级 tauri 改了命名，这条断言会先失败，提醒同步门禁逻辑。
  assert.match(workflow, /dmgs\[@\]/, '应按数组遍历实际 DMG 文件');
});

test('本地触发脚本存在，且三种入口都指向同一个 workflow', () => {
  for (const scriptPath of [
    'scripts/build-cloud.command',
    'scripts/build-cloud.ps1',
    '构建云端包.bat',
  ]) {
    assert.ok(fs.existsSync(path.join(root, scriptPath)), `${scriptPath} 应该存在`);
  }

  const sh = read('scripts/build-cloud.command');
  const ps1 = read('scripts/build-cloud.ps1');
  const bat = read('构建云端包.bat');

  // 两个脚本都必须触发 build.yml 的 workflow_dispatch，并支持附到 Release。
  // 写法各异（shell 用数组含 workflow/run，ps1 用 @('workflow','run',...)），
  // 因此这里只要求三个关键信息同时出现，不锁具体语法。
  for (const [name, source] of [['build-cloud.command', sh], ['build-cloud.ps1', ps1]]) {
    assert.match(source, /build\.yml/, `${name} 应触发 build.yml`);
    assert.match(source, /\bworkflow\b[\s\S]{0,40}\brun\b/, `${name} 应调用 gh workflow run`);
    assert.match(source, /release_tag/, `${name} 应支持 release_tag 输入`);
  }

  // 双击入口只是包装 PowerShell 脚本，参数要透传。
  assert.match(bat, /scripts\\build-cloud\.ps1/, 'bat 应调用 build-cloud.ps1');
  assert.match(bat, /%(\*|1)/, 'bat 应把参数透传给 ps1');
});

test('两个触发脚本用同一条「只有大版本才自动出包」的判定', () => {
  const sh = read('scripts/build-cloud.command');
  const ps1 = read('scripts/build-cloud.ps1');

  // 小版本（x.y.z 中 y 或 z 非 0）不应随手占用一次全矩阵构建。
  // 两份脚本必须各自独立实现同一个判定 —— 它们在不同平台上运行，不能共享代码。
  assert.match(sh, /MAJOR_PATCH="\$\{VERSION#\*\.\}"/
    , 'shell 版应从版本号里切出 minor.patch 部分');
  assert.match(sh, /"\$\{MAJOR_PATCH\}" != "0\.0"/, 'shell 版应只在 x.0.0 时放行');
  assert.match(ps1, /IndexOf\('\.'\) \+ 1/, 'ps1 版应从版本号里切出 minor.patch 部分');
  assert.match(ps1, /\$minorPatch -ne '0\.0'/, 'ps1 版应只在 x.0.0 时放行');

  // 两份都要留一个显式覆盖口，否则紧急发小版本就没有出路。
  assert.match(sh, /--force/, 'shell 版应提供 --force');
  assert.match(ps1, /\[switch\]\$Force/, 'ps1 版应提供 -Force');
});

test('本地脚本默认推到能写 Actions 的 fork，而不是只有拉取权限的上游', () => {
  // origin 是上游作者仓库，本仓库对它是只读的；推送与 Actions 都在 fork 上。
  const sh = read('scripts/build-cloud.command');
  const ps1 = read('scripts/build-cloud.ps1');
  for (const [name, source] of [['build-cloud.command', sh], ['build-cloud.ps1', ps1]]) {
    assert.match(source, /macos-fork/, `${name} 应优先识别 fork 远端`);
    assert.match(source, /origin/, `${name} 应把 origin 作为回退`);
  }
  // 下载目录要与 .gitignore 的忽略项一致，避免把几 GB 产物加进版本库。
  // .gitignore 在 Windows 检出同样是 CRLF，所以这里也必须归一后再匹配行首尾。
  const ignore = readLines('.gitignore');
  assert.match(ignore, /^dist-cloud$/m, '.gitignore 应忽略 dist-cloud 下载目录');
  assert.match(sh, /dist-cloud/, 'shell 版下载到 dist-cloud');
  assert.match(ps1, /dist-cloud/, 'ps1 版下载到 dist-cloud');
});
