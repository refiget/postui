#!/bin/sh

set -eu

DEFAULT_RELEASE_URL="https://github.com/refiget/postui/releases/latest/download"
release_archive_name=

usage() {
    cat <<'EOF'
用法:
  ./install.sh [选项]

选项:
  --prefix DIR       安装目录，默认是 ~/.local/share/postui
  --archive-url URL  使用指定发布包地址（优先于本地包和版本号）
  --version VERSION 安装指定发布版本，例如 0.1.2（默认最新版本）
  --skip-init        安装后不执行 postui init
  -h, --help         显示帮助

环境变量:
  POSTUI_INSTALL_DIR  等同于 --prefix
  POSTUI_ARCHIVE_URL  等同于 --archive-url
  POSTUI_SKIP_INIT=1  等同于 --skip-init
  POSTUI_VERSION     等同于 --version
EOF
}

die() {
    printf '%s\n' "postui 安装失败: $*" >&2
    exit 1
}

command_exists() {
    command -v "$1" >/dev/null 2>&1
}

copy_file() {
    source_path=$1
    destination_path=$2
    mode=$3

    if [ "$source_path" = "$destination_path" ]; then
        chmod "$mode" "$destination_path"
        return
    fi
    install -m "$mode" "$source_path" "$destination_path"
}

download_package() {
    archive_url=$1
    command_exists mktemp || die "本地没有 mktemp 命令"
    command_exists tar || die "本地没有 tar 命令"
    work_dir=$(mktemp -d "${TMPDIR:-/tmp}/postui-install.XXXXXX")
    archive_path=$work_dir/postui.tar.gz
    extract_dir=$work_dir/package
    mkdir -p "$extract_dir"

    printf '%s\n' "正在下载发布包: $archive_url"
    if command_exists curl; then
        curl --fail --location --retry 2 --connect-timeout 15 --output "$archive_path" "$archive_url"
    elif command_exists wget; then
        wget --quiet --output-document "$archive_path" "$archive_url"
    else
        die "本地没有 postui.bin，且未找到 curl 或 wget"
    fi

    tar -xzf "$archive_path" -C "$extract_dir"
    binary_path=$(find "$extract_dir" -type f -name postui.bin -print -quit)
    [ -n "$binary_path" ] || die "发布包中没有 postui.bin"
    package_dir=$(dirname "$binary_path")
}

check_platform() {
    case "$(uname -s):$(uname -m)" in
        Linux:x86_64|Linux:amd64)
            release_archive_name=postui-linux-amd64.tar.gz
            ;;
        Darwin:x86_64|Darwin:amd64)
            release_archive_name=postui-macos-amd64.tar.gz
            ;;
        Darwin:arm64|Darwin:aarch64)
            release_archive_name=postui-macos-arm64.tar.gz
            ;;
        *)
            die "只支持 Linux amd64、macOS Intel 和 macOS Apple Silicon，当前平台是 $(uname -s) $(uname -m)"
            ;;
    esac
}

check_platform

if [ -z "${HOME:-}" ]; then
    die "无法确定 HOME"
fi

archive_url=${POSTUI_ARCHIVE_URL:-}
version=${POSTUI_VERSION:-}
skip_init=${POSTUI_SKIP_INIT:-0}
data_home=${XDG_DATA_HOME:-$HOME/.local/share}
install_dir=${POSTUI_INSTALL_DIR:-$data_home/postui}
package_dir=
work_dir=

cleanup() {
    if [ -n "$work_dir" ]; then
        rm -rf "$work_dir"
    fi
}
trap cleanup EXIT HUP INT TERM

while [ "$#" -gt 0 ]; do
    case "$1" in
        --prefix|--install-dir)
            [ "$#" -ge 2 ] || die "$1 需要一个目录"
            install_dir=$2
            shift 2
            ;;
        --archive-url)
            [ "$#" -ge 2 ] || die "--archive-url 需要一个地址"
            archive_url=$2
            shift 2
            ;;
        --version)
            [ "$#" -ge 2 ] || die "--version 需要一个版本号"
            version=$2
            shift 2
            ;;
        --skip-init)
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

case "$skip_init" in
    0|1) ;;
    *) die "POSTUI_SKIP_INIT 只能是 0 或 1" ;;
esac

use_local_package=0
if [ -z "$archive_url" ] && [ -z "$version" ]; then
    use_local_package=1
fi
if [ -z "$archive_url" ]; then
    if [ -n "$version" ]; then
        version=${version#v}
        case "$version" in
            ''|*[!0-9A-Za-z.-]*) die "无效版本号: $version" ;;
        esac
        archive_url="https://github.com/refiget/postui/releases/download/v$version/$release_archive_name"
    else
        archive_url=$DEFAULT_RELEASE_URL/$release_archive_name
    fi
fi

if [ -z "$package_dir" ]; then
    case "$0" in
        */*) local_dir=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd) ;;
        *) local_dir= ;;
    esac
    if [ -n "$local_dir" ] && [ "$use_local_package" -eq 1 ] && [ -f "$local_dir/postui.bin" ]; then
        package_dir=$local_dir
    else
        download_package "$archive_url"
    fi
fi

binary_path=$package_dir/postui.bin
[ -f "$binary_path" ] || die "发布目录中没有 postui.bin: $package_dir"
command_exists install || die "系统没有 install 命令"
command_exists cp || die "系统没有 cp 命令"
command_exists mkdir || die "系统没有 mkdir 命令"

mkdir -p "$install_dir"
install_dir=$(CDPATH='' cd -- "$install_dir" && pwd)

copy_file "$binary_path" "$install_dir/postui.bin" 755
if [ -f "$package_dir/postui" ]; then
    copy_file "$package_dir/postui" "$install_dir/postui" 755
else
    cat >"$install_dir/postui" <<'EOF'
#!/bin/sh

set -eu
package_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
exec "$package_dir/postui.bin" "$@"
EOF
    chmod 755 "$install_dir/postui"
fi


if [ "$skip_init" -eq 0 ]; then
    if [ -z "${SHELL:-}" ]; then
        if command_exists zsh; then
            SHELL=$(command -v zsh)
        elif command_exists bash; then
            SHELL=$(command -v bash)
        else
            die "无法识别当前 shell，请设置 SHELL=/bin/bash 或 SHELL=/bin/zsh 后重试"
        fi
        export SHELL
    fi
    case "$SHELL" in
        */bash|*/zsh) ;;
        *) die "postui init 只支持 bash 或 zsh，当前 SHELL=$SHELL" ;;
    esac
    "$install_dir/postui" init
fi

printf '%s\n' "postui 已安装到: $install_dir"
if [ "$skip_init" -eq 0 ]; then
    printf '%s\n' "请重新打开终端，或执行上面 init 输出的 source 命令。"
fi
