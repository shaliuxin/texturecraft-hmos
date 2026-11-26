# TextureCraft i18n 修复：界面与材质名全面中文化（可同步上游）

本文档记录两个相关但独立的 i18n 缺陷及其修复，供同步到上游 `storytold/texturecraft`。
两处修复都遵循项目既有的 `tr()`/`n_()` 翻译约定，零新增依赖、零破坏性改动。

---

## 一、缺陷 1：界面整体英文（catalog 为空）

### 根因
`crates/ui-egui/i18n_build.rs` 在 `CRAFT_I18N_DIR` 未设置时，生成的 `craft_i18n.rs` 是空
catalog（`&[]`）。运行时 `i18n.rs` 的 `I18n::new(&[], …)` 对任何 `set_language("zh-Hans")`
都返回 false，回退到 index 0（English），于是整个界面是英文。

上游各语种翻译其实**已经存在**，但被写成 `crates/ui-egui/src/i18n/<lang>.rs` 里的
`TABLE: &[(&str, &str)]` 数组，而 `i18n_build.rs` 只认 `CRAFT_I18N_DIR/locales/<code>/texturecraft.tsv`
这种 TSV 布局，**两条路径从未打通**，导致编译期永远拿不到翻译。

### 修复
1. 新增 `scripts/gen_i18n_tsv.py`：把 9 个语种的 `TABLE` 数组（ar/de/es/fr/it/ja/ko/pt-BR/zh-Hans）
   解析成 `locales/<code>/texturecraft.tsv`（每语种 1053 条 UI 文案）。用字符级状态机解析
   Rust 字符串字面量，正确处理多行/转义/raw string。
2. `scripts/build_native.sh` 在编译前跑该脚本并 `export CRAFT_I18N_DIR="$ROOT"` +
   `CRAFT_I18N_REQUIRED=1`，让 `i18n_build.rs` 把 TSV 嵌入二进制。
3. 运行期语言检测链：ArkTS `i18n.System.getSystemLocale()` → JSON `systemLocale` →
   Rust `locale::install()` 写 `LANG/LANGUAGE` → `sys_locale::get_locale()` →
   `language_for_locale("zh-Hans-CN")` → `"zh-Hans"`。`lib.rs` 里当 `config.language`
   映射不到时，回退用 `config.system_locale`。

### 同步上游
- 上游若已有 `locales/` 目录，只需补 `gen_i18n_tsv.py`（或把 `TABLE` 数组直接落成 TSV）。
- 推荐上游统一用 TSV 布局（而非 `.rs` 的 `TABLE`），`i18n_build.rs` 已支持。

---

## 二、缺陷 2：材质名/遮罩/笔刷/Alpha/环境名仍英文

### 根因
缺陷 1 修好后，UI 文案（菜单、按钮、面板标题）已中文化，但**资产库货架（Assets shelf）**里
的**材质名、智能遮罩名、笔刷预设名、Alpha 库名、光照环境名**仍是英文。

根因：这些名字是引擎侧的**数据驱动静态标签**，定义在
`crates/engine/src/materials.rs`、`crates/engine/src/smart_masks.rs`、
`crates/paint/src/presets.rs`、`crates/paint/src/alphas.rs`、`crates/core/src/project.rs`，
**不属于 UI 的 `TABLE` 翻译集合**；而且 UI 展示点在 `shelf_entry.rs` 的 `label()`/`matches()`
里**直接返回原始 `material.label`，没走 `tr()`**（对比 `Filter`/`Procedural` 分支已经走了）。

即：即使翻译表里有这些词条，展示点不调用 `tr()` 也不会翻译；而翻译表里又确实没有这些词条。
两个环节都缺，所以一直显示英文。

### 修复
**代码侧（展示点接入 `tr()`，可同步上游）** —— `patches/i18n-data-labels.diff`：

`crates/ui-egui/src/shelf_entry.rs`
- `label()`：`Material/SmartMask/Brush/Alpha/Environment` 分支由 `x.label` 改为 `tr(x.label)`
  （`SavedBrush`/`Image` 是用户数据，保持原样）。
- `matches()`：搜索文本数组为上述分支补 `tr(...)` 变体，使中文搜索也能命中。

`crates/ui-egui/src/dock.rs`
- 笔刷预设弹窗（`brush_presets_popover`）：`caption(category)` → `caption(tr(category))`、
  `selectable_label(false, preset.label)` → `tr(preset.label)`。
- 材质链接属性分组标题（`link_properties`）：`p.group.to_uppercase()` →
  `tr(p.group.as_str()).to_uppercase()`。

> `tr()` 找不到译文时回退英文源串（`i18n.rs` 第 89-93 行），所以**未翻译的词条自动安全回退**，
> 不会崩溃、不会留空。

> 类型注意：`SmartMaterial.label`/`SmartMask.label`/`BrushPreset.label` 是 `&'static str`，
> 可直接 `tr(x.label)`；而 `LibraryAlpha.label` 与 `LinkParam.group` 是 `String`，
> 需 `tr(x.label.as_str())`（`&String` 传给 `tr(&str)` 不一定触发 Deref 收窄，用 `as_str()` 最稳妥）。

**翻译数据侧（TSV，随 `gen_i18n_tsv.py` 生成）**：
- `gen_i18n_tsv.py` 新增 `ZH`（简体中文，全量）与 `OTHER`（fr/de/es/it/ja/ko/ar/pt-BR 的分类+环境名）
  两个翻译表，并入各语种 TSV。
- 简体中文全量覆盖：77 个材质名 + 7 个材质分类 + 24 个智能遮罩 + 18 个笔刷预设 + 31 个 Alpha + 6 个环境。
- 其它语种：分类名（Metal→Métal…）+ 环境名已译；逐资产名回退英文（可后续补充）。
- 仅当 `译文 ≠ 英文源` 时才写 TSV 行，保持 catalog 干净。
- **去重**：craft-i18n 以英文源串为唯一 key，数据标签里 `Square/Grunge/Star/Copper` 等 8 个
  同时是 UI 文案，会与 UI TABLE 撞车触发 `check_all()` 的 "defined twice" 报错。
  生成器合并 UI+数据条目时按 source 去重（UI 优先），规避该报错。

### 验证
- zh-Hans TSV 由 1053 行增至 1231 行（+178 条数据标签）；`Metal→金属`、`Steel→钢`、
  `橡木(Oak)→橡木`、`柔光摄影棚(Studio Soft)` 等已写入。
- 真机：资产库货架材质名、分类、遮罩、笔刷、Alpha、环境均随系统语言显示中文。

---

## 三、同类问题全面排查（其它模块）

排查范围：`crates/ui-egui/src/**` 中所有 `.label` / `.category` / `.group` 展示点是否绕过 `tr()`。

| 位置 | 是否已翻译 | 处理 |
|---|---|---|
| `shelf_entry.rs::label()` 材质/遮罩/笔刷/Alpha/环境 | ❌ 原为原始 | ✅ 已修（本补丁） |
| `shelf_entry.rs::matches()` 搜索数组 | ❌ 原缺 `tr` 变体 | ✅ 已修（本补丁） |
| `dock.rs::brush_presets_popover` 分类/名 | ❌ 原为原始 | ✅ 已修（本补丁） |
| `dock.rs::link_properties` 分组标题 | ❌ 原为原始 | ✅ 已修（本补丁） |
| `dock.rs:1554` 通道填充源 `f.label()` | 技术值（如格式名） | 保留（非文案） |
| `dock.rs:2665/2670` 链接参数名 `p.label` | 小众（材质链接编辑器） | 建议后续补 `tr()` |
| `levels_editor.rs:125` 数值句柄 `handle.label()` | 数值/百分比，非文案 | 无需翻译 |
| `shelf_folders.rs:51-52` 分类/环境 | ✅ 已 `tr(segment)` | 只需补 TSV 词条 |
| `dock.rs` 属性/遮罩/笔刷/图版面板 39 条提示 | ✅ 已 `tr()`，缺 TSV 词条 | ✅ 已补中文（见下） |

结论：**主要用户可见的数据驱动标签已全部接入 `tr()`**；剩余为技术值或小众编辑器，
可按需跟进。

### 属性面板提示文案（`dock.rs` 的 `tr()` 调用但缺 TSV 词条）

第二轮排查发现：`dock.rs` 的属性 / 遮罩 / 笔刷 / 图版面板有 39 处 `tr("...")` 调用，其
key 不在 UI `TABLE` 里，`gen_i18n_tsv.py` 也未收录，于是 `tr()` 回退英文（如属性面板的
`No properties found`、`Select a layer to paint on`、`Parameters`、`Roundness` 等）。

修复：在 `gen_i18n_tsv.py` 的 `ZH` 字典新增「属性面板提示」分区，补入这 39 条简体中文
译文。仅中文全量覆盖，其它语种仍回退英文（缺失自动安全回退）。
zh-Hans 数据条目 178 → 218；因 craft-i18n 按 source casefold 去重，与 UI `TABLE`
撞车的 key（如 `Parameters`）以 UI 条目优先，不触发 "defined twice"。

### 新建项目对话框文案（`new_project_advanced.rs` + `project_form.rs`）

第三轮排查发现：`File > New`（新建项目）高级对话框与项目表单里的若干 `tr()`/`trf()` 文案
（如「未选择文件」「文档分辨率」「色彩管理」「自动展 UV」「UV 平铺设置（UDIM）」「导入设置」
「物理尺寸」等）其 key 既不在 UI `TABLE` 也不在 `ZH` 字典，于是 `tr()` 回退英文。

修复：在 `gen_i18n_tsv.py` 的 `ZH` 字典新增两个分区——
- 「新建项目对话框」（`crates/ui-egui/src/new_project_advanced.rs`）
- 「项目表单辅助」（`crates/ui-egui/src/project_form.rs`：切空间 / 物理尺寸 / 材质通道说明等）
共补 52 条简体中文译文。zh-Hans 数据条目 218 → 269；无源串以 `...` 结尾，省略号铁律不适用；
生成器 `check_all()` 无 "defined twice" / 省略号报错。

---

## 四、同步到上游的操作清单

1. 应用代码补丁（二选一）：
   - `git apply patches/i18n-data-labels.diff`（在 `texturecraft` 仓库根目录，已做 reverse-check）
   - 或直接改 `shelf_entry.rs::label()`/`matches()` 与 `dock.rs` 两处，按本补丁 diff。
2. 把数据驱动标签加入翻译源（任选其一）：
   - 沿用本移植的 TSV 方案：用 `scripts/gen_i18n_tsv.py` 生成 `locales/<code>/texturecraft.tsv`，
     构建时设 `CRAFT_I18N_DIR` + `CRAFT_I18N_REQUIRED=1`。
   - 或把 `ZH`/`OTHER` 词条直接并入上游各语种 `TABLE` 数组。
3. 其它语种逐资产名翻译：在 `OTHER` 对应语种补 `材质名/遮罩名/笔刷名/Alpha 名` 的译文即可，
   缺失项自动回退英文，不影响编译。
4. 构建验证：`cargo build`（设好 `CRAFT_I18N_DIR`）后跑 i18n 单测
   `crates/ui-egui/tests/i18n_build.rs`，确认 `check_all()` 无新增缺失键。

---

## 五、产物

- `patches/i18n-data-labels.diff` —— 代码侧修复补丁（`shelf_entry.rs` + `dock.rs`，4.6 KB）。
- `scripts/gen_i18n_tsv.py` —— TSV 生成器（含数据标签翻译表：材质/遮罩/笔刷/Alpha/环境 +
  属性面板 39 条 + 新建对话框 52 条）。
- `locales/<code>/texturecraft.tsv` —— 9 语种 catalog（zh-Hans 1053 UI + 269 数据标签，
  去重后 1300 行；`gen_i18n_tsv.py` 当前输出：`zh-Hans: 1053 UI + 269 data entries`，
  合计 9 语种 9764 条）。
