#!/bin/sh

set -eu

usage() {
    cat <<'EOF'
用法:
  ./package-linux.sh [选项]

选项:
  --output-dir DIR    输出目录，默认是项目根目录 打包区
  -h, --help          显示帮助
EOF
}

die() {
    printf '%s\n' "PostUI 打包失败: $*" >&2
    exit 1
}

require_file() {
    [ -f "$1" ] || die "找不到$2: $1"
}

project_root=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
default_output_dir=$project_root/打包区
output_dir=$default_output_dir

while [ "$#" -gt 0 ]; do
    case "$1" in
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

require_file "$project_root/install.sh" "安装脚本"
command -v cargo >/dev/null 2>&1 || die "找不到 cargo"
command -v tar >/dev/null 2>&1 || die "找不到 tar"
command -v mktemp >/dev/null 2>&1 || die "找不到 mktemp"

target=x86_64-unknown-linux-musl
printf '%s\n' "构建 Linux amd64 release: $target"
cargo build --locked --manifest-path "$project_root/Cargo.toml" --release --target "$target"

binary_path=$project_root/target/$target/release/postui
require_file "$binary_path" "Linux release 二进制"

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

for optional_directory in docs; do
    if [ -d "$project_root/$optional_directory" ]; then
        cp -R "$project_root/$optional_directory" "$package_dir/$optional_directory"
    fi
done

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
for optional_directory in docs; do
    rm -rf "$output_dir/$optional_directory"
    if [ -d "$package_dir/$optional_directory" ]; then
        cp -R "$package_dir/$optional_directory" "$output_dir/$optional_directory"
    fi
done

archive_path=$output_dir/postui-linux-amd64.tar.gz
tar -czf "$archive_path" -C "$package_dir" .
printf '%s\n' "Linux 发布包已生成: $archive_path"
