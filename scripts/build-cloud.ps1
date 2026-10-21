# 手动触发云端全平台打包（Windows PowerShell 5.1 及以上）。
#
# 这个脚本**不打包**，它只是让 GitHub Actions 去打：调用 build.yml 的
# workflow_dispatch，由云端出 Windows / Linux / macOS(双架构) / Android 四套产物。
#
# 自动打包只在推 v* tag 时发生。日常想验证就用本脚本手动触发；-PushTag 只在
# 「大版本」（x.0.0）时才允许，避免小改动随手就占用一次全矩阵。
#
# 用法（也可直接双击仓库根目录的 构建云端包.bat）：
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts\build-cloud.ps1
#   ... -Watch                      触发并跟到结束
#   ... -ReleaseTag v3.10.0         触发并附到该 Release
#   ... -PushTag                    打 tag（仅大版本）→ 自动出包 + Release
#   ... -DryRun                     只打印将要执行的命令
#
# macOS / Linux 用户请用 scripts/build-cloud.command。

param(
    [string]$ReleaseTag = '',
    [string]$Ref = '',
    [string]$Repository = '',
    [switch]$PushTag,
    [switch]$Watch,
    [switch]$DryRun,
    [switch]$Yes,
    [switch]$Force
)

$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8

$Workflow = 'build.yml'
$root = Split-Path -Parent $PSScriptRoot
Push-Location $root
try {
    function Fail([string]$Message) {
        Write-Host "错误: $Message" -ForegroundColor Red
        exit 1
    }

    # ---- 依赖检查 ---------------------------------------------------------
    foreach ($tool in @('git', 'gh', 'node')) {
        if (-not (Get-Command $tool -ErrorAction SilentlyContinue)) {
            Fail "缺少 $tool。gh 请从 https://cli.github.com 安装，Node.js 请从 https://nodejs.org 安装。"
        }
    }
    gh auth status *> $null
    if ($LASTEXITCODE -ne 0) {
        Fail 'gh 未登录。请先执行 gh auth login（需要 repo 与 workflow 权限）。'
    }

    # ---- 目标仓库 ---------------------------------------------------------
    # origin 是上游，本 fork 才有推送与 Actions 权限，因此优先用 macos-fork 远端。
    if (-not $Repository) {
        foreach ($remote in @('macos-fork', 'origin')) {
            $url = (& git config --get "remote.$remote.url" 2>$null)
            if ($LASTEXITCODE -ne 0 -or -not $url) { continue }
            $slug = $url -replace '^https?://[^/]+/', '' -replace '^git@[^:]+:', '' -replace '\.git$', ''
            if ($slug -match '^[^/]+/[^/]+$') { $Repository = $slug; break }
        }
    }
    if (-not $Repository) { Fail '无法确定目标仓库，请用 -Repository owner/name 指定' }

    # ---- 目标 ref ---------------------------------------------------------
    # workflow_dispatch 要求 workflow 文件已存在于目标 ref，默认用默认分支。
    if (-not $Ref) {
        $Ref = (& gh repo view $Repository --json defaultBranchRef --jq '.defaultBranchRef.name' 2>$null)
        if ($LASTEXITCODE -ne 0 -or -not $Ref) { $Ref = 'master' }
    }

    $version = (& node -p "require('./package.json').version")
    if ($LASTEXITCODE -ne 0 -or $version -notmatch '^\d+\.\d+\.\d+$') {
        Fail "package.json 的版本号不是 x.y.z: $version"
    }

    Write-Host "仓库:   $Repository"
    Write-Host "分支:   $Ref"
    Write-Host "版本:   $version"
    Write-Host ''

    # ---- 模式一：打 tag（仅大版本）→ 自动出包 + Release --------------------
    if ($PushTag) {
        $tag = "v$version"
        $minorPatch = $version.Substring($version.IndexOf('.') + 1)
        # 「大版本」定义为 x.0.0：只有这种版本才允许打 tag 自动出全平台包。
        if ($minorPatch -ne '0.0' -and -not $Force) {
            Fail "只有大版本（x.0.0）才自动出包，当前是 $version。`n      日常验证请直接运行本脚本（不带 -PushTag）手动触发；`n      确实要为当前版本发版，请加 -Force。"
        }
        & git rev-parse -q --verify "refs/tags/$tag" *> $null
        if ($LASTEXITCODE -eq 0) {
            Fail "tag $tag 已存在。若要重出该版本的包，请用 -ReleaseTag $tag 手动触发。"
        }

        Write-Host "将推送 tag $tag 到 $Repository，这会自动触发全平台打包并创建 Release。"
        if ($DryRun) {
            Write-Host "[dry-run] git tag $tag && git push <fork> $tag"
            exit 0
        }
        if (-not $Yes) {
            $answer = Read-Host '确认继续? [y/N]'
            if ($answer -notmatch '^(y|yes)$') { Write-Host '已取消'; exit 0 }
        }

        & git tag $tag
        if ($LASTEXITCODE -ne 0) { Fail "创建 tag 失败: $tag" }
        $forkRemote = ''
        foreach ($remote in @('macos-fork', 'origin')) {
            & git config --get "remote.$remote.url" *> $null
            if ($LASTEXITCODE -eq 0) { $forkRemote = $remote; break }
        }
        if (-not $forkRemote) { Fail '找不到可推送的远端' }
        & git push $forkRemote $tag
        if ($LASTEXITCODE -ne 0) { Fail "推送 tag 失败: $tag" }
        Write-Host ''
        Write-Host "已推送 $tag。查看进度："
        Write-Host "  gh run list --repo $Repository --workflow=$Workflow --limit 3"
        exit 0
    }

    # ---- 模式二：手动触发 workflow_dispatch -------------------------------
    $ghArgs = @('workflow', 'run', $Workflow, '--repo', $Repository, '--ref', $Ref)
    if ($ReleaseTag) { $ghArgs += @('--field', "release_tag=$ReleaseTag") }

    Write-Host '将要执行:'
    Write-Host "  gh $($ghArgs -join ' ')"
    if ($DryRun) { exit 0 }

    $beforeId = (& gh run list --repo $Repository --workflow=$Workflow --limit 1 --json databaseId --jq '.[0].databaseId // 0' 2>$null)
    if ($LASTEXITCODE -ne 0 -or -not $beforeId) { $beforeId = '0' }

    & gh @ghArgs
    if ($LASTEXITCODE -ne 0) { Fail '触发失败（检查 workflow 是否已存在于目标分支）' }
    Write-Host ''
    Write-Host '已触发。产物在 Actions 页面：'
    Write-Host "  https://github.com/$Repository/actions/workflows/$Workflow"

    if ($Watch) {
        # workflow_dispatch 是异步的，run 记录要过一会儿才出现，轮询等待新 run。
        Write-Host ''
        Write-Host '等待运行记录出现...'
        $runId = ''
        for ($i = 0; $i -lt 30; $i++) {
            $candidate = (& gh run list --repo $Repository --workflow=$Workflow --limit 1 --json databaseId --jq '.[0].databaseId // 0' 2>$null)
            if ($LASTEXITCODE -eq 0 -and $candidate -and $candidate -ne '0' -and $candidate -ne $beforeId) {
                $runId = $candidate; break
            }
            Start-Sleep -Seconds 5
        }
        if (-not $runId) { Fail '等不到新的运行记录，请手动打开 Actions 页面查看' }
        Write-Host "跟踪运行 $runId（Ctrl+C 可退出，构建本身不受影响）"
        & gh run watch $runId --repo $Repository --exit-status
        $status = $LASTEXITCODE
        if ($status -ne 0) {
            Write-Host ''
            Write-Host "构建未全部成功（退出码 $status）。查看失败详情："
            Write-Host "  gh run view $runId --repo $Repository --log-failed"
            exit $status
        }
        Write-Host ''
        Write-Host '全部平台构建成功。下载产物：'
        Write-Host "  gh run download $runId --repo $Repository -D dist-cloud"
    }
}
finally {
    Pop-Location
}
