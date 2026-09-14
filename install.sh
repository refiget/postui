#!/usr/bin/env bash

set -Eeuo pipefail

readonly REPOSITORY="refiget/postui"
readonly DEFAULT_RELEASE_URL="https://github.com/${REPOSITORY}/releases/latest/download"

archive_url=${POSTUI_ARCHIVE_URL:-}
version=${POSTUI_VERSION:-}
skip_init=${POSTUI_SKIP_INIT:-${POSTUI_NO_MODIFY_PATH:-0}}
home=${HOME:-}
data_home=
install_dir=
release_archive_name=
release_label=
target_label=
source_kind=
script_dir=
package_dir=
binary_path=
package_version=
work_dir=
install_stage_dir=
shell_path=
shell_rc=

usage() {
    cat <<'EOF'
用法:
  ./install.sh [选项]

选项:
  --prefix DIR       安装目录，默认是 ~/.local/share/postui
  --archive-url URL  发布包地址
  --version VERSION  指定版本，例如 0.1.2（默认 latest）
  --skip-init        跳过 PATH 配置
  --no-modify-path   --skip-init 的别名
  -h, --help         显示帮助

环境变量:
  POSTUI_INSTALL_DIR       等同于 --prefix
  POSTUI_ARCHIVE_URL       等同于 --archive-url
  POSTUI_VERSION           等同于 --version
  POSTUI_SKIP_INIT=1       等同于 --skip-init
  POSTUI_NO_MODIFY_PATH=1  等同于 --skip-init
EOF
}

die() {
    printf '%s\n' "安装失败: $*" >&2
    exit 1
}

command_exists() {
    command -v "$1" >/dev/null 2>&1
}

require_command() {
    command_exists "$1" || die "找不到命令: $1"
}

cleanup() {
    if [[ -n ${install_stage_dir:-} && -d ${install_stage_dir:-} ]] && command_exists rm; then
        rm -rf "$install_stage_dir"
    fi
    if [[ -n ${work_dir:-} && -d ${work_dir:-} ]] && command_exists rm; then
        rm -rf "$work_dir"
    fi
}

trap cleanup EXIT

parse_args() {
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --prefix|--install-dir)
                [[ $# -ge 2 ]] || die "$1 需要一个目录"
                install_dir=$2
                shift 2
                ;;
            --archive-url)
                [[ $# -ge 2 ]] || die "--archive-url 需要一个地址"
                archive_url=$2
                shift 2
                ;;
            --version)
                [[ $# -ge 2 ]] || die "--version 需要一个版本号"
                version=$2
                shift 2
                ;;
            --skip-init|--no-modify-path)
                skip_init=1
                shift
                ;;
            -h|--help)
                usage
                exit 0
                ;;
            *)
                die "未知参数: $1"
                ;;
        esac
    done
}

validate_input() {
    [[ -n "$home" ]] || die "无法确定 HOME"
    [[ -n "$install_dir" ]] || die "安装目录不能为空"

    case "$skip_init" in
        0|1) ;;
        *) die "POSTUI_SKIP_INIT 只能是 0 或 1" ;;
    esac

    if [[ -n "$version" ]]; then
        version=${version#v}
        [[ -n "$version" ]] || die "版本号不能为空"
        [[ "$version" != *[!0-9A-Za-z.-]* ]] || die "无效版本号: $version"
    fi
}

detect_platform() {
    require_command uname

    local system machine
    system=$(uname -s)
    machine=$(uname -m)

    case "$system:$machine" in
        Linux:x86_64|Linux:amd64)
            release_archive_name="postui-linux-amd64.tar.gz"
            target_label="x86_64-unknown-linux-musl"
            ;;
        Darwin:x86_64|Darwin:amd64)
            release_archive_name="postui-macos-amd64.tar.gz"
            target_label="x86_64-apple-darwin"
            ;;
        Darwin:arm64|Darwin:aarch64)
            release_archive_name="postui-macos-arm64.tar.gz"
            target_label="aarch64-apple-darwin"
            ;;
        *)
            die "不支持的平台: $system $machine"
            ;;
    esac
}

resolve_script_dir() {
    local script_path=${BASH_SOURCE[0]:-}
    if [[ -n "$script_path" && -f "$script_path" ]]; then
        script_dir=$(cd "$(dirname "$script_path")" && pwd -P)
    fi
}

resolve_source() {
    if [[ -z "$archive_url" && -z "$version" && -n "$script_dir" && -f "$script_dir/postui.bin" ]]; then
        package_dir=$script_dir
        source_kind="local"
        release_label="local"
        return
    fi

    source_kind="release"
    if [[ -n "$archive_url" ]]; then
        release_label="custom"
    elif [[ -n "$version" ]]; then
        archive_url="https://github.com/${REPOSITORY}/releases/download/v${version}/${release_archive_name}"
        release_label="$version"
    else
        archive_url="${DEFAULT_RELEASE_URL}/${release_archive_name}"
        release_label="latest"
    fi
}

download_archive() {
    local url=$1
    local output=$2

    if command_exists curl; then
        if ! curl \
            --fail \
            --location \
            --retry 3 \
            --retry-delay 1 \
            --connect-timeout 15 \
            --progress-bar \
            --output "$output" \
            "$url"; then
            die "下载失败"
        fi
    elif command_exists wget; then
        if ! wget \
            --tries=3 \
            --timeout=15 \
            --progress=bar:force:noscroll \
            --output-document="$output" \
            "$url"; then
            die "下载失败"
        fi
    else
        die "找不到 curl 或 wget"
    fi
}

download_package() {
    require_command mktemp
    require_command mkdir
    require_command tar
    require_command find
    require_command dirname

    work_dir=$(mktemp -d "${TMPDIR:-/tmp}/postui-install.XXXXXX")
    local archive_path="$work_dir/postui.tar.gz"
    local extract_dir="$work_dir/package"
    mkdir -p "$extract_dir"

    printf '  url: %s\n' "$archive_url"
    download_archive "$archive_url" "$archive_path"
    tar -xzf "$archive_path" -C "$extract_dir" || die "解压失败"

    binary_path=$(find "$extract_dir" -type f -name postui.bin -print -quit)
    [[ -n "$binary_path" ]] || die "发布包缺少 postui.bin"
    package_dir=$(dirname "$binary_path")
}

validate_package() {
    require_command chmod
    [[ -n "$package_dir" && -f "$package_dir/postui.bin" ]] || die "发布包缺少 postui.bin"

    binary_path="$package_dir/postui.bin"
    chmod 755 "$binary_path"
    if ! package_version=$("$binary_path" --version 2>&1); then
        die "版本校验失败"
    fi
    case "$package_version" in
        postui\ *) ;;
        *) die "版本校验失败" ;;
    esac

    if [[ -n "$version" && "$package_version" != "postui $version"* ]]; then
        die "版本不匹配"
    fi
    printf '  version: %s\n' "$package_version"
}

create_wrapper() {
    local target=$1
    printf '%s\n' \
        '#!/usr/bin/env bash' \
        'set -Eeuo pipefail' \
        "script_dir=\"\$(cd \"\$(dirname \"\${BASH_SOURCE[0]}\")\" && pwd -P)\"" \
        "exec \"\$script_dir/postui.bin\" \"\$@\"" >"$target"
    chmod 755 "$target"
}

install_package() {
    require_command install
    require_command mkdir
    require_command mktemp
    require_command mv
    require_command chmod
    require_command rm

    if [[ -e "$install_dir" && ! -d "$install_dir" ]]; then
        die "安装目录无效"
    fi
    mkdir -p "$install_dir"
    install_dir=$(cd "$install_dir" && pwd -P)

    [[ ! -d "$install_dir/postui.bin" ]] || die "安装目标无效"
    [[ ! -d "$install_dir/postui" ]] || die "安装目标无效"

    install_stage_dir=$(mktemp -d "$install_dir/.postui-install.XXXXXX")
    install -m 755 "$binary_path" "$install_stage_dir/postui.bin"
    if [[ -f "$package_dir/postui" ]]; then
        install -m 755 "$package_dir/postui" "$install_stage_dir/postui"
    else
        create_wrapper "$install_stage_dir/postui"
    fi

    "$install_stage_dir/postui.bin" --version >/dev/null 2>&1 || die "安装校验失败"
    mv -f "$install_stage_dir/postui.bin" "$install_dir/postui.bin"
    mv -f "$install_stage_dir/postui" "$install_dir/postui"
    rm -rf "$install_stage_dir"
    install_stage_dir=
}

verify_installation() {
    "$install_dir/postui" --version >/dev/null 2>&1 || die "启动校验失败"
}

resolve_shell() {
    shell_path=${SHELL:-}
    if [[ -z "$shell_path" ]]; then
        if [[ -f "$home/.zshrc" && ! -f "$home/.bashrc" ]]; then
            shell_path="zsh"
        elif [[ -f "$home/.bashrc" && ! -f "$home/.zshrc" ]]; then
            shell_path="bash"
        else
            die "无法识别 shell"
        fi
    fi

    case "$shell_path" in
        bash|*/bash)
            shell_rc="$home/.bashrc"
            ;;
        zsh|*/zsh)
            shell_rc="$home/.zshrc"
            ;;
        *)
            die "不支持的 shell"
            ;;
    esac
}

configure_shell() {
    resolve_shell
    SHELL="$shell_path" "$install_dir/postui" init >/dev/null 2>&1 || die "PATH 配置失败"
}

print_path_command() {
    printf "echo 'export PATH=\"%s:\$PATH\"' >> \"%s\"\n" "$install_dir" "$shell_rc"
}

home=${HOME:-}
[[ -n "$home" ]] || die "无法确定 HOME"
data_home=${XDG_DATA_HOME:-$home/.local/share}
install_dir=${POSTUI_INSTALL_DIR:-$data_home/postui}

parse_args "$@"
validate_input
detect_platform
resolve_script_dir
resolve_source

if [[ "$source_kind" == "local" ]]; then
    printf 'using local package %s\n' "$package_dir"
else
    printf 'downloading postui %s %s\n' "$release_label" "$target_label"
    download_package
fi

printf 'installing to %s\n' "$install_dir"
validate_package
install_package
verify_installation
printf 'everything'\''s installed!\n'

if [[ "$skip_init" == 0 ]]; then
    configure_shell
    printf 'To add %s to your PATH, either restart your shell or run:\n' "$install_dir"
    printf '  source "%s"\n' "$shell_rc"
    printf '  '
    print_path_command
else
    printf 'PATH modification skipped\n'
fi
