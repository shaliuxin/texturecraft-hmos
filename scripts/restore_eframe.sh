#!/usr/bin/env bash
# 把 third_party/eframe 里打过 OHOS 补丁的 eframe 覆盖回 cargo 的 git 检出目录。
#
# 背景（见 移植报告.md / 通用清单 §35）：
#   eframe 在适配层 Cargo.toml 里是 git 依赖（storytold/egui rev 6d518fc7…），
#   其 OHOS 补丁（epi.rs / lib.rs 用 #[cfg(target_env="ohos")] 门控掉 mod native /
#   run_native / create_native / UserEvent，并让 egui-wgpu 不编译 winit 模块）只能落在
#   cargo 的 git 检出缓存里：
#     ~/.cargo/git/checkouts/egui-<hash>/<rev>/crates/eframe/
#   一旦 git 缓存被清（换机器、`rm -rf ~/.cargo/git`、`cargo update` 重拉）这些补丁会丢失，
#   编译又退回「winit 不支持 ohos」的 compile_error。
#
# 因此把补丁后的 eframe 源**以仓库里的 third_party/eframe 为权威副本**，本脚本在每次构建前
# 把权威副本覆盖回 git 检出缓存，做到「清缓存后一条命令即可复原」。
#
# 用法：  bash scripts/restore_eframe.sh
# 退出码：0 成功；3 检出目录缺失（需先 cargo fetch 拉取 git 依赖再跑本脚本）。
set -euo pipefail

# 仓库根 = 本脚本上级目录的上级
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
SRC="$ROOT/third_party/eframe"

# 与 native/rust/Cargo.toml 中 eframe 的 rev 保持一致；cargo 检出目录用短 hash 名，
# 故按完整 rev 的前缀匹配目录名（默认取前 7 位，与 cargo 默认短 hash 一致）。
REV="6d518fc799681571963842f95b55a6b68431b15a"
REV_PREFIX="${REV:0:7}"

if [ ! -d "$SRC" ]; then
  echo "[restore_eframe] 权威副本缺失：$SRC" >&2
  exit 4
fi

# cargo git 检出根：<hash>/ 目录下有 <短hash>/crates/eframe
CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}"
CHECKOUT_ROOT="$CARGO_HOME/git/checkouts"

# cargo 检出目录名是完整 rev 的短前缀（默认 7 位，可能更长）。
# 遍历所有 <egui-*>/<其它仓> 下的子目录，命中「目录名是完整 rev 的前缀」即为目标。
DEST=""
if [ -d "$CHECKOUT_ROOT" ]; then
  for base in "$CHECKOUT_ROOT"/*/; do
    for sub in "$base"*/; do
      [ -d "$sub" ] || continue
      dname="$(basename "$sub")"
      # 目录名是完整 rev 的前缀（且长度 >=6）
      case "$REV" in
        "$dname"*)
          len=${#dname}
          if [ "$len" -ge 6 ] && [ -d "$sub/crates/eframe" ]; then
            DEST="$sub/crates/eframe"
            break 2
          fi
          ;;
      esac
    done
  done
fi

if [ -z "$DEST" ]; then
  echo "[restore_eframe] 未在 $CHECKOUT_ROOT 找到 rev=$REV 的 eframe 检出。" >&2
  echo "[restore_eframe] 请先在 native/rust 下执行一次 cargo fetch（需网络）：" >&2
  echo "                 cd native/rust && cargo fetch --target aarch64-unknown-linux-ohos" >&2
  echo "[restore_eframe] 再重跑本脚本。" >&2
  exit 3
fi

echo "[restore_eframe] 源: $SRC"
echo "[restore_eframe] 目标(覆盖): $DEST"

# 覆盖源码 + Cargo.toml + data（不动 .cargo-ok / .git，cargo 用 rev 索引识别，不依赖工作区脏状态）
cp -f "$SRC/Cargo.toml" "$DEST/Cargo.toml"
cp -rf "$SRC/src" "$DEST/"
cp -rf "$SRC/data" "$DEST/" 2>/dev/null || true
cp -f "$SRC/README.md" "$DEST/README.md" 2>/dev/null || true
cp -f "$SRC/CHANGELOG.md" "$DEST/CHANGELOG.md" 2>/dev/null || true

# 校验关键补丁标记是否到位
if grep -q 'target_env = "ohos"' "$DEST/src/epi.rs" && \
   grep -q 'target_env = "ohos"' "$DEST/src/lib.rs"; then
  echo "[restore_eframe] OK：eframe OHOS 补丁已就绪于 $DEST"
  exit 0
else
  echo "[restore_eframe] 失败：目标未检出 OHOS 补丁标记，请检查源 $SRC" >&2
  exit 5
fi
