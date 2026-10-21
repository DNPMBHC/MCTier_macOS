#!/usr/bin/env bash
# 手动触发云端全平台打包（macOS / Linux）。
#
# 这个脚本**不打包**，它只是让 GitHub Actions 去打：调用 build.yml 的
# workflow_dispatch，由云端出 Windows / Linux / macOS(双架构) / Android 四套产物。
#
# 自动打包只在推 v* tag 时发生。日常想验证就用本脚本手动触发；本脚本的
# --push-tag 只在「大版本」（x.0.0）时才允许，避免小改动随手就占用一次全矩阵。
#
# 用法：
#   ./scripts/build-cloud.command                     # 触发一次，产物留在 Actions
#   ./scripts/build-cloud.command --watch             # 触发并跟到结束
#   ./scripts/build-cloud.command --release-tag v3.10.0   # 触发并附到该 Release
#   ./scripts/build-cloud.command --push-tag          # 打 tag（仅大版本）→ 自动出包 + Release
#   ./scripts/build-cloud.command --dry-run           # 只打印将要执行的命令
#
# Windows 用户请用 scripts/build-cloud.bat。

set -Eeuo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd -- "${SCRIPT_DIR}/.." && pwd)"
cd "${ROOT}"

WORKFLOW="build.yml"
REPO=""
REF=""
RELEASE_TAG=""
PUSH_TAG=0
WATCH=0
DRY_RUN=0
ASSUME_YES=0
FORCE=0

log()  { printf '%s\n' "$*"; }
fail() { printf '错误: %s\n' "$*" >&2; exit 1; }

usage() {
  # BSD（macOS）的 sed 不支持 `--`，这里直接传文件路径。范围覆盖文件头注释块。
  sed -n '2,17p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
  exit 0
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --release-tag)
      [[ -n "${2:-}" ]] || fail '--release-tag 需要参数，例如 --release-tag v3.10.0'
      RELEASE_TAG="$2"; shift 2 ;;
    --ref)
      [[ -n "${2:-}" ]] || fail '--ref 需要参数，例如 --ref master'
      REF="$2"; shift 2 ;;
    --repo)
      [[ -n "${2:-}" ]] || fail '--repo 需要参数，例如 --repo DNPMBHC/MCTier_macOS'
      REPO="$2"; shift 2 ;;
    --push-tag) PUSH_TAG=1; shift ;;
    --watch) WATCH=1; shift ;;
    --dry-run) DRY_RUN=1; shift ;;
    --yes|-y) ASSUME_YES=1; shift ;;
    --force) FORCE=1; shift ;;
    -h|--help) usage ;;
    *) fail "未知参数: $1（用 --help 查看用法）" ;;
  esac
done

# ---- 依赖检查 -------------------------------------------------------------
command -v git >/dev/null 2>&1 || fail '缺少 git'
command -v gh  >/dev/null 2>&1 || fail '缺少 GitHub CLI（gh）。macOS: brew install gh；Linux: 见 https://cli.github.com'
gh auth status >/dev/null 2>&1 || fail 'gh 未登录。请先执行 gh auth login（需要 repo 与 workflow 权限）'
command -v node >/dev/null 2>&1 || fail '缺少 Node.js（用于读取 package.json 里的版本号）'

# ---- 目标仓库 -------------------------------------------------------------
# origin 是上游，本 fork 才有推送与 Actions 权限，因此优先用 macos-fork 远端。
if [[ -z "${REPO}" ]]; then
  for remote in macos-fork origin; do
    url="$(git config --get "remote.${remote}.url" 2>/dev/null || true)"
    [[ -n "${url}" ]] || continue
    REPO="$(printf '%s' "${url}" | sed -E 's#^https?://[^/]+/##; s#^git@[^:]+:##; s#\.git$##')"
    [[ "${REPO}" == */* ]] && break
    REPO=""
  done
fi
[[ -n "${REPO}" ]] || fail '无法确定目标仓库，请用 --repo owner/name 指定'

# ---- 目标 ref -------------------------------------------------------------
# workflow_dispatch 要求 workflow 文件已经存在于目标 ref 上，因此默认用默认分支。
if [[ -z "${REF}" ]]; then
  REF="$(gh repo view "${REPO}" --json defaultBranchRef --jq '.defaultBranchRef.name' 2>/dev/null || true)"
  [[ -n "${REF}" ]] || REF="master"
fi

VERSION="$(node -p "require('./package.json').version")"
[[ "${VERSION}" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || fail "package.json 的版本号不是 x.y.z: ${VERSION}"

log "仓库:   ${REPO}"
log "分支:   ${REF}"
log "版本:   ${VERSION}"
log ""

# ---- 模式一：打 tag（仅大版本）→ 自动出包 + Release -------------------------
if (( PUSH_TAG )); then
  TAG="v${VERSION}"
  MAJOR_PATCH="${VERSION#*.}"
  # 「大版本」定义为 x.0.0：只有这种版本才允许打 tag 自动出全平台包。
  if [[ "${MAJOR_PATCH}" != "0.0" ]] && (( FORCE == 0 )); then
    fail "只有大版本（x.0.0）才自动出包，当前是 ${VERSION}。
      日常验证请直接运行本脚本（不带 --push-tag）手动触发；
      确实要为当前版本发版，请加 --force。"
  fi
  if git rev-parse -q --verify "refs/tags/${TAG}" >/dev/null; then
    fail "tag ${TAG} 已存在。若要重出该版本的包，请用 --release-tag ${TAG} 手动触发。"
  fi

  log "将推送 tag ${TAG} 到 ${REPO}，这会自动触发全平台打包并创建 Release。"
  if (( DRY_RUN )); then
    log "[dry-run] git tag ${TAG} && git push <fork> ${TAG}"
    exit 0
  fi
  if (( ASSUME_YES == 0 )) && [[ -t 0 ]]; then
    read -r -p '确认继续? [y/N] ' answer
    case "${answer}" in y|Y|yes) ;; *) log '已取消'; exit 0 ;; esac
  fi

  git tag "${TAG}"
  # 显式推到 fork：origin 是上游，没有推送权限。
  fork_remote=""
  for remote in macos-fork origin; do
    if git config --get "remote.${remote}.url" >/dev/null 2>&1; then fork_remote="${remote}"; break; fi
  done
  [[ -n "${fork_remote}" ]] || fail '找不到可推送的远端'
  git push "${fork_remote}" "${TAG}"
  log ""
  log "已推送 ${TAG}。查看进度："
  log "  gh run list --repo ${REPO} --workflow=${WORKFLOW} --limit 3"
  exit 0
fi

# ---- 模式二：手动触发 workflow_dispatch -----------------------------------
GH_ARGS=(workflow run "${WORKFLOW}" --repo "${REPO}" --ref "${REF}")
if [[ -n "${RELEASE_TAG}" ]]; then
  GH_ARGS+=(--field "release_tag=${RELEASE_TAG}")
fi

log '将要执行:'
log "  gh ${GH_ARGS[*]}"
if (( DRY_RUN )); then exit 0; fi

before_id="$(gh run list --repo "${REPO}" --workflow="${WORKFLOW}" --limit 1 --json databaseId --jq '.[0].databaseId // 0' 2>/dev/null || echo 0)"

gh "${GH_ARGS[@]}"
log ''
log '已触发。产物在 Actions 页面：'
log "  https://github.com/${REPO}/actions/workflows/${WORKFLOW}"

if (( WATCH )); then
  # workflow_dispatch 是异步的，run 记录要过一会儿才出现，轮询等待新 run。
  log ''
  log '等待运行记录出现...'
  run_id=""
  for _ in $(seq 1 30); do
    run_id="$(gh run list --repo "${REPO}" --workflow="${WORKFLOW}" --limit 1 --json databaseId --jq '.[0].databaseId // 0' 2>/dev/null || echo 0)"
    if [[ -n "${run_id}" && "${run_id}" != "0" && "${run_id}" != "${before_id}" ]]; then break; fi
    run_id=""
    sleep 5
  done
  [[ -n "${run_id}" ]] || fail '等不到新的运行记录，请手动打开 Actions 页面查看'
  log "跟踪运行 ${run_id}（Ctrl+C 可退出，构建本身不受影响）"
  gh run watch "${run_id}" --repo "${REPO}" --exit-status || {
    status=$?
    log ''
    log "构建未全部成功（退出码 ${status}）。查看失败详情："
    log "  gh run view ${run_id} --repo ${REPO} --log-failed"
    exit "${status}"
  }
  log ''
  log '全部平台构建成功。下载产物：'
  log "  gh run download ${run_id} --repo ${REPO} -D dist-cloud"
fi
