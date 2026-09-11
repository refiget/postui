#!/bin/sh

set -eu

DEFAULT_ARCHIVE_URL="https://github.com/refiget/postui/releases/latest/download/postui-linux-amd64.tar.gz"

usage() {
    cat <<'EOF'
用法:
  ./install.sh [选项]

选项:
  --prefix DIR       安装目录，默认是 ~/.local/share/postui
  --archive-url URL  本地没有二进制时使用的发布包地址
  --skip-init        安装后不执行 postui init
  --force-config     用发布包中的配置覆盖已存在的配置
  -h, --help         显示帮助

环境变量:
  POSTUI_INSTALL_DIR  等同于 --prefix
  POSTUI_ARCHIVE_URL  等同于 --archive-url
  POSTUI_SKIP_INIT=1  等同于 --skip-init
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

copy_if_needed() {
    source_path=$1
    destination_path=$2
    mode=$3

    if [ -e "$destination_path" ]; then
        [ -f "$destination_path" ] || die "安装路径不是文件: $destination_path"
        if [ "$force_config" -ne 1 ]; then
            return
        fi
    fi
    copy_file "$source_path" "$destination_path" "$mode"
}

copy_directory_contents() {
    source_path=$1
    destination_path=$2
    overwrite=$3

    [ -d "$source_path" ] || die "发布目录中没有目录: $source_path"
    if [ -e "$destination_path" ] && [ ! -d "$destination_path" ]; then
        die "安装路径不是目录: $destination_path"
    fi
    mkdir -p "$destination_path"

    for source_entry in "$source_path"/* "$source_path"/.[!.]* "$source_path"/..?*; do
        [ -e "$source_entry" ] || continue
        entry_name=${source_entry##*/}
        destination_entry=$destination_path/$entry_name
        if [ -d "$source_entry" ]; then
            if [ -e "$destination_entry" ] && [ ! -d "$destination_entry" ]; then
                die "安装路径不是目录: $destination_entry"
            fi
            (copy_directory_contents "$source_entry" "$destination_entry" "$overwrite")
        elif [ -f "$source_entry" ]; then
            if [ -e "$destination_entry" ] && [ "$overwrite" -ne 1 ]; then
                continue
            fi
            (copy_file "$source_entry" "$destination_entry" 644)
        fi
    done
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
    [ "$(uname -s)" = "Linux" ] || die "只支持 Linux"
    case "$(uname -m)" in
        x86_64|amd64) ;;
        *) die "只支持 Linux amd64 (x86_64)，当前架构是 $(uname -m)" ;;
    esac
}

check_platform

if [ -z "${HOME:-}" ]; then
    die "无法确定 HOME"
fi

archive_url=${POSTUI_ARCHIVE_URL:-$DEFAULT_ARCHIVE_URL}
skip_init=${POSTUI_SKIP_INIT:-0}
force_config=0
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
        --skip-init)
            skip_init=1
            shift
            ;;
        --force-config)
            force_config=1
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

if [ -z "$package_dir" ]; then
    case "$0" in
        */*) local_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd) ;;
        *) local_dir=$(pwd) ;;
    esac
    if [ -f "$local_dir/postui.bin" ]; then
        package_dir=$local_dir
    else
        download_package "$archive_url"
    fi
fi

binary_path=$package_dir/postui.bin
config_path=$package_dir/config.yaml
project_config_path=$package_dir/.postui/config.yaml
collections_dir=$package_dir/.postui/collections

[ -f "$binary_path" ] || die "发布目录中没有 postui.bin: $package_dir"
[ -f "$config_path" ] || die "发布目录中没有 config.yaml: $package_dir"
[ -f "$project_config_path" ] || die "发布目录中没有 .postui/config.yaml: $package_dir"
[ -d "$collections_dir" ] || die "发布目录中没有 .postui/collections: $package_dir"
command_exists install || die "系统没有 install 命令"
command_exists cp || die "系统没有 cp 命令"
command_exists mkdir || die "系统没有 mkdir 命令"

mkdir -p "$install_dir"
install_dir=$(CDPATH= cd -- "$install_dir" && pwd)

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

copy_if_needed "$config_path" "$install_dir/config.yaml" 644
copy_directory_contents "$package_dir/.postui" "$install_dir/.postui" "$force_config"
if [ -d "$package_dir/test_files" ]; then
    copy_directory_contents "$package_dir/test_files" "$install_dir/test_files" 0
fi
if [ -d "$package_dir/themes" ]; then
    copy_directory_contents "$package_dir/themes" "$install_dir/themes" 0
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
