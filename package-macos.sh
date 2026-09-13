#!/bin/sh

set -eu

usage() {
    cat <<'EOF'
用法:
  ./package-macos.sh [选项]

选项:
  --target TARGET      Rust target，默认按当前 macOS 架构选择
  --output-dir DIR     输出目录，默认是项目根目录 打包区
  -h, --help           显示帮助

支持的 target:
  x86_64-apple-darwin  macOS Intel
  aarch64-apple-darwin macOS Apple Silicon
EOF
}

die() {
    printf '%s\n' "PostUI macOS 打包失败: $*" >&2
    exit 1
}

require_file() {
    [ -f "$1" ] || die "找不到$2: $1"
}

require_command() {
    command -v "$1" >/dev/null 2>&1 || die "找不到命令: $1"
}

project_root=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
default_output_dir=$project_root/打包区
output_dir=$default_output_dir
target=

while [ "$#" -gt 0 ]; do
    case "$1" in
        --target)
            [ "$#" -ge 2 ] || die "--target 需要一个 target"
            target=$2
            shift 2
            ;;
        --output-dir)
            [ "$#" -ge 2 ] || die "--output-dir 需要一个目录"
            output_dir=$2
            shift 2
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *) die "未知参数: $1" ;;
    esac
done

if [ -z "$target" ]; then
    case "$(uname -s):$(uname -m)" in
        Darwin:x86_64|Darwin:amd64) target=x86_64-apple-darwin ;;
        Darwin:arm64|Darwin:aarch64) target=aarch64-apple-darwin ;;
        *) die "无法从当前系统识别 macOS target，请显式传入 --target" ;;
    esac
fi

case "$target" in
    x86_64-apple-darwin) archive_suffix=amd64 ;;
    aarch64-apple-darwin) archive_suffix=arm64 ;;
    *) die "不支持的 macOS target: $target" ;;
esac

require_file "$project_root/install.sh" "安装脚本"
require_command cargo
require_command tar
require_command mktemp
require_command install

printf '%s\n' "构建 macOS release: $target"
cargo build --locked --manifest-path "$project_root/Cargo.toml" --release --target "$target"

binary_path=$project_root/target/$target/release/postui
require_file "$binary_path" "macOS release 二进制"

work_dir=$(mktemp -d "${TMPDIR:-/tmp}/postui-package.XXXXXX")
trap 'rm -rf "$work_dir"' EXIT HUP INT TERM
package_dir=$work_dir/package
mkdir -p "$package_dir"

install -m 755 "$binary_path" "$package_dir/postui.bin"
install -m 755 "$project_root/install.sh" "$package_dir/install.sh"
install -m 644 "$project_root/README.md" "$package_dir/README.md"

cat >"$package_dir/postui" <<'EOF'
#!/bin/sh

set -eu
package_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
exec "$package_dir/postui.bin" "$@"
EOF
chmod 755 "$package_dir/postui"

if [ -d "$project_root/docs" ]; then
    cp -R "$project_root/docs" "$package_dir/docs"
fi

mkdir -p "$output_dir"
output_dir=$(CDPATH= cd -- "$output_dir" && pwd)
managed_marker=$output_dir/.postui-package-output
if [ ! -f "$managed_marker" ] && [ "$output_dir" != "$default_output_dir" ]; then
    first_entry=$(find "$output_dir" -mindepth 1 -maxdepth 1 -print -quit)
    [ -z "$first_entry" ] || die "自定义输出目录非空且不是 PostUI 打包目录: $output_dir"
fi
: >"$managed_marker"

install -m 755 "$package_dir/postui" "$output_dir/postui"
install -m 755 "$package_dir/postui.bin" "$output_dir/postui.bin"
install -m 755 "$package_dir/install.sh" "$output_dir/install.sh"
install -m 644 "$package_dir/README.md" "$output_dir/README.md"
rm -rf "$output_dir/docs"
if [ -d "$package_dir/docs" ]; then
    cp -R "$package_dir/docs" "$output_dir/docs"
fi

archive_path=$output_dir/postui-macos-$archive_suffix.tar.gz
tar -czf "$archive_path" -C "$package_dir" .
printf '%s\n' "macOS 发布包已生成: $archive_path"
