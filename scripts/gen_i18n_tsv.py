#!/usr/bin/env python3
"""Convert i18n language module TABLE arrays to TSV catalog files for craft-i18n.

Parses Rust source character-by-character to handle multi-line string literals.
"""
import os
import sys

I18N_DIR = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "texturecraft", "crates", "ui-egui", "src", "i18n")
LOCALES_DIR = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "locales")

MODULES = {
    "ar.rs": "ar",
    "de.rs": "de",
    "es.rs": "es",
    "fr.rs": "fr",
    "it.rs": "it",
    "ja.rs": "ja",
    "ko.rs": "ko",
    "pt_br.rs": "pt-BR",
    "zh_hans.rs": "zh-Hans",
}

def extract_string_pairs(rs_text: str):
    """Extract (source, translation) pairs from a Rust TABLE array.
    
    Uses a character-level state machine to handle multi-line string literals.
    """
    # Find "pub const TABLE" 
    idx = rs_text.find("pub const TABLE")
    if idx < 0:
        return []
    
    # Find the first "[" after "="
    eq_idx = rs_text.find("=", idx)
    if eq_idx < 0:
        return []
    
    # Find the opening "[" of the array
    arr_start = rs_text.find("[", eq_idx)
    if arr_start < 0:
        return []
    
    # Now parse the array: [( "src", "tr" ), ( "src", "tr" ), ...]
    i = arr_start + 1
    pairs = []
    
    while i < len(rs_text):
        # Skip whitespace and commas
        while i < len(rs_text) and rs_text[i] in ' \t\n\r,':
            i += 1
        
        if i >= len(rs_text) or rs_text[i] == ']':
            break
        
        if rs_text[i] != '(':
            # Skip unexpected character
            i += 1
            continue
        
        i += 1  # skip '('
        
        # Skip whitespace
        while i < len(rs_text) and rs_text[i] in ' \t\n\r':
            i += 1
        
        # Should be '""' or raw string
        if i >= len(rs_text) or rs_text[i] != '"':
            i += 1
            continue
        
        # Parse first string literal
        src, i = parse_rust_string(rs_text, i)
        
        # Skip to ','
        while i < len(rs_text) and rs_text[i] != ',':
            i += 1
        i += 1  # skip ','
        
        # Skip whitespace
        while i < len(rs_text) and rs_text[i] in ' \t\n\r':
            i += 1
        
        # Parse second string literal
        if i >= len(rs_text) or rs_text[i] != '"':
            break
        tr, i = parse_rust_string(rs_text, i)
        
        # Skip to ')'
        while i < len(rs_text) and rs_text[i] != ')':
            i += 1
        
        if src and tr:
            pairs.append((src, tr))
    
    return pairs

def parse_rust_string(text, i):
    """Parse a Rust string literal starting at position i (which is the opening quote).
    
    Returns (unescaped_string, new_position).
    Handles: "normal", "escaped", r"raw", r#"raw"#, r##"raw"##
    """
    # Check for raw string
    if i + 1 < len(text) and text[i + 1] == 'r':
        # r"..." or r#"..."# or r##"..."##
        j = i + 2
        hashes = 0
        while j < len(text) and text[j] == '#':
            hashes += 1
            j += 1
        if j < len(text) and text[j] == '"':
            j += 1  # skip opening quote
            start = j
            # Find closing "# (hashes + quote)
            closing = '#' * hashes + '"'
            end = text.find(closing, start)
            if end < 0:
                return "", i + 1
            raw = text[start:end]
            return raw, end + len(closing)
    
    # Normal string "..."
    if text[i] != '"':
        return "", i + 1
    
    i += 1  # skip opening quote
    result = []
    while i < len(text):
        c = text[i]
        if c == '\\':
            if i + 1 < len(text):
                nc = text[i + 1]
                if nc == 'n':
                    result.append('\n')
                elif nc == 't':
                    result.append('\t')
                elif nc == '\\':
                    result.append('\\')
                elif nc == '"':
                    result.append('"')
                elif nc == 'r':
                    result.append('\r')
                elif nc == '0':
                    result.append('\0')
                else:
                    result.append(nc)
                i += 2
            else:
                i += 1
        elif c == '"':
            i += 1  # skip closing quote
            return ''.join(result), i
        else:
            result.append(c)
            i += 1
    
    return ''.join(result), i

def escape_tsv(s: str) -> str:
    """Escape for TSV: craft-i18n uses \\t and \\n."""
    return s.replace('\\', '\\\\').replace('\t', '\\t').replace('\n', '\\n')

# ---------------------------------------------------------------------------
# Data-driven labels (material/mask/brush/alpha names, categories, lighting
# environments). These live in the engine crates as static catalog keys but are
# not part of the UI-chrome i18n module TABLEs, so the UI previously showed them
# in English. After wrapping the display sites with tr() in shelf_entry.rs and
# dock.rs, these rows make them follow the chosen language (falling back to the
# English source when a locale has no translation for a given name).
#
# zh-Hans gets full coverage; the other locales get the shared category and
# environment names (the most visible groupings) and fall back to English for the
# per-asset names.
# ---------------------------------------------------------------------------

# English -> Simplified Chinese, full coverage of the data-driven labels.
ZH = {
    # Material shelf categories (crates/engine/src/materials.rs).
    "Metal": "金属", "Wood": "木材", "Stone & Masonry": "石材与砌体",
    "Plastic & Rubber": "塑料与橡胶", "Fabric & Leather": "织物与皮革",
    "Ceramic & Glass": "陶瓷与玻璃", "Organic": "有机物",
    # Metals
    "Steel": "钢", "Steel Stainless": "不锈钢", "Steel Dark": "暗色钢",
    "Iron Raw": "生铁", "Cast Iron": "铸铁", "Copper": "铜",
    "Copper Hammered": "锤纹铜", "Copper Oxidized": "氧化铜",
    "Brass": "黄铜", "Brass Tarnished": "氧化黄铜", "Bronze": "青铜",
    "Bronze Statue": "青铜雕像", "Gold Polished": "抛光金",
    "Gold Dirty": "脏金", "Silver": "银", "Aluminium Raw": "生铝",
    "Aluminium Brushed": "拉丝铝", "Aluminium Scratched": "划痕铝",
    "Steel Brushed Anisotropic": "各向异性拉丝钢", "Chrome": "铬",
    "Chrome Smudged": "污渍铬", "Titanium": "钛", "Nickel": "镍",
    "Platinum": "铂", "Zinc": "锌", "Cobalt": "钴",
    "Steel Galvanized": "镀锌钢", "Steel Water Stained": "水渍钢",
    "Steel Painted Worn": "磨损漆面钢", "Steel Painted Rust Leaks": "渗锈漆面钢",
    "Rust": "铁锈", "Iron Rusted": "生锈铁",
    # Woods
    "Wood Varnished": "上漆木材", "Oak": "橡木", "Walnut": "胡桃木",
    "Pine": "松木", "Plywood": "胶合板", "Bark": "树皮", "Maple": "枫木",
    "Cherry": "樱桃木", "Bamboo": "竹", "Wood Painted": "油漆木",
    "Wood Chipped Paint": "掉漆木材", "Wood Weathered": "风化木材",
    # Stone, masonry and earth
    "Concrete": "混凝土", "Concrete Dirty": "脏混凝土",
    "Brick Red": "红砖", "Brick Mossy": "长满青苔的砖",
    "Tiles Ceramic": "陶瓷砖", "Marble": "大理石", "Marble Black": "黑大理石",
    "Granite": "花岗岩", "Cobblestone": "鹅卵石", "Sandstone": "砂岩",
    "Terrazzo": "水磨石", "Slate": "板岩", "Stone Mossy": "长满青苔的石头",
    "Asphalt": "沥青", "Gravel": "砾石", "Sand": "沙", "Mud": "泥土",
    # Ceramic and glass
    "Ceramic White": "白瓷", "Glass Clear": "透明玻璃",
    "Glass Frosted": "磨砂玻璃",
    # Plastic and rubber
    "Rubber": "橡胶", "Plastic Dusty": "积灰塑料",
    "Plastic Glossy": "亮光塑料", "Plastic Grimy": "污垢塑料",
    "Plastic Matte": "哑光塑料", "Plastic Scuffed": "刮花塑料",
    "Plastic Textured": "纹理塑料", "Carbon Fiber": "碳纤维",
    "Car Paint": "车漆",
    # Fabric and leather
    "Denim": "牛仔布", "Fabric Cotton": "棉布", "Leather": "皮革",
    "Leather Black": "黑皮革", "Leather Worn": "磨损皮革",
    "Linen": "亚麻", "Velvet": "天鹅绒", "Wool": "羊毛",
    # Organic and misc
    "Bone": "骨头", "Paper": "纸", "Plaster": "石膏",
    "Candle Wax": "蜡烛", "Clay Earthenware": "陶土", "Cork": "软木",
    "Moss": "苔藓", "Ice": "冰", "Snow": "雪", "Skin": "皮肤",
    "Mossy": "长满青苔", "Military Paint": "军绿漆",
    "Paint Sun Faded": "日晒褪色漆",
    # Smart masks (crates/engine/src/smart_masks.rs).
    "Edges Worn": "磨损边缘", "Edges Chipped": "崩边",
    "Dirt in Cavities": "凹陷积灰", "Dust from Top": "顶部落尘",
    "Ground Dirt": "地面泥土", "Rust Leaks": "渗锈", "Scratches": "划痕",
    "Grunge Blotches": "污斑", "Paint Chips": "掉漆",
    "Fingerprints": "指纹", "Edges Soft": "柔和边缘",
    "Edges Sharp": "锐利边缘", "Exposed Areas": "裸露区域",
    "Thin Areas": "薄处", "Dust in Cavities": "凹陷积尘",
    "Moss from Top": "顶部苔藓", "Water Stains": "水渍",
    "Sun Faded": "日晒褪色", "Snow from Top": "顶部积雪",
    "Moss from Ground": "地面苔藓", "Rust in Cavities": "凹陷积锈",
    "Edge Scratches": "边缘划痕", "Dirt Streaks": "污渍条纹",
    "Occluded Areas": "遮蔽区域",
    # Brush preset categories (crates/paint/src/presets.rs).
    "Basic": "基础", "Wear and grunge": "磨损与污渍", "Details": "细节",
    # Brush preset names.
    "Basic hard": "硬质基础笔", "Basic soft": "软质基础笔",
    "Airbrush": "喷枪", "Square": "方笔", "Dots": "点笔",
    "Dry brush": "干笔", "Chalk": "粉笔", "Spray": "喷涂",
    "Grunge": "污渍笔", "Splatter": "飞溅", "Drips": "滴落",
    "Fur": "毛发", "Rivets": "铆钉", "Stitches": "缝线",
    "Stars": "星形", "Chips": "碎屑", "Cracks": "裂纹",
    # Alpha library categories (crates/paint/src/alphas.rs).
    "Grunge spots": "污点", "Hard surface": "硬表面",
    "Stencil letters": "镂空字母",
    # Alpha library names.
    "Soft round": "柔和圆形", "Hard round": "硬圆形",
    "Rectangle": "矩形", "Triangle": "三角形", "Hexagon": "六边形",
    "Ring": "圆环", "Cross": "十字", "Star": "星形", "Sparkle": "闪光",
    "Star 6": "六芒星", "Arrow": "箭头", "Dots grid": "点阵",
    "Sponge": "海绵", "Grunge spots": "污点", "Speckle": "斑点",
    "Fur strands": "毛束", "Leaf": "叶子", "Wood knot": "木结",
    "Rivet": "铆钉", "Bolt head": "螺栓头", "Screw slot": "一字螺丝",
    "Screw cross": "十字螺丝", "Vent slots": "通风槽",
    "Fabric stamp": "织物印章", "Bricks stamp": "砖块印章",
    # Lighting environments (crates/core/src/project.rs).
    "Studio Soft": "柔光摄影棚", "Studio Contrast": "高对比摄影棚",
    "Overcast": "阴天", "Sunset": "日落", "Warehouse": "仓库", "Night": "夜晚",
    # Properties / mask / brush / stencil panel hints (crates/ui-egui/src/dock.rs).
    # These are tr() calls in the properties panel whose keys are not in the UI
    # TABLE, so without these rows tr() fell back to the English source.
    "No properties found": "未找到属性",
    "Select a layer to paint on": "选择一个要绘制到的图层",
    "No project is open": "未打开任何项目",
    "Parameters": "参数",
    "Blending options": "混合选项",
    "Blending mode of the effect": "效果的混合模式",
    "Apply blending mode to all channels": "将混合模式应用到所有通道",
    "Apply opacity to all channels": "将不透明度应用到所有通道",
    "Apply both to all channels": "将两者应用到所有通道",
    "Copy channel blending settings": "复制通道混合设置",
    "Paste channel blending settings": "粘贴通道混合设置",
    "Channels dragged": "拖动的通道",
    "These are the brush's material channels, the same ones Paint writes and the Material tab shows. A channel the selected layer does not have is skipped.":
        "这些是笔刷的材质通道，与画笔写入、材质页显示的是同一组；所选图层没有的通道会被跳过。",
    "Solo layer": "仅显示该图层",
    "Solo layer without its mask": "仅显示该图层（不含遮罩）",
    "Alt+click: show only this layer. Alt+Shift+click: without its mask.":
        "Alt+单击：仅显示该图层。Alt+Shift+单击：不含其遮罩。",
    "Create smart mask": "创建智能遮罩",
    "Copy mask to clipboard": "复制遮罩到剪贴板",
    "Mask copied to the clipboard": "遮罩已复制到剪贴板",
    "Export mask to file...": "导出遮罩到文件...",
    "Position X": "位置 X", "Position Y": "位置 Y",
    "Size X": "大小 X", "Size Y": "大小 Y",
    "Flip U": "翻转 U", "Flip V": "翻转 V",
    "Flip the copy's image": "翻转复制的图像",
    "Mirror across": "沿轴镜像",
    "Keep proportions": "保持比例",
    "Width and height follow each other by the image's aspect": "宽高按图像比例互相联动",
    "Roundness": "圆角",
    "Reset stencil": "重置图版",
    "Put the stencil back in the middle of the view at its starting size and rotation":
        "将图版还原到视图中央的初始大小与旋转",
    "Drop an image from the Assets panel here to use it as the stencil":
        "从资产面板拖一张图到这里作为图版",
    "S + left-drag rotates (with Shift in 15° steps), S + right-drag scales, S + middle-drag moves; or use Position X / Y.":
        "S+左拖旋转（加 Shift 按 15° 步进），S+右拖缩放，S+中拖移动；或使用位置 X / Y。",
    "Use this brush (keeps its size)": "使用此笔刷（保留其大小）",
    "Ctrl+Shift+right-drag: size and hardness in fine steps.":
        "Ctrl+Shift+右拖：以细步长调整大小和硬度。",
    "Aligned": "已对齐",
    "Not aligned: every stroke starts at the source. V + click sets a new source.":
        "未对齐：每一笔都从源点起笔。V+单击设定新源点。",
    "On: the offset from the first stroke is kept. Off: every stroke starts at the source point.":
        "开启：保留与第一笔的偏移。关闭：每一笔都从源点起笔。",
}

# Shared groupings the non-Chinese locales translate; per-asset names fall back
# to the English source. Keys are the English category/environment names.
OTHER = {
    "fr": {
        "Metal": "Métal", "Wood": "Bois", "Stone & Masonry": "Pierre et maçonnerie",
        "Plastic & Rubber": "Plastique et caoutchouc", "Fabric & Leather": "Tissu et cuir",
        "Ceramic & Glass": "Céramique et verre", "Organic": "Organique",
        "Basic": "De base", "Wear and grunge": "Usure et salissure", "Details": "Détails",
        "Hard surface": "Surface dure", "Stencil letters": "Lettres pochoir",
        "Studio Soft": "Studio doux", "Studio Contrast": "Studio contraste",
        "Overcast": "Nuageux", "Sunset": "Coucher de soleil", "Warehouse": "Entrepôt", "Night": "Nuit",
    },
    "de": {
        "Metal": "Metall", "Wood": "Holz", "Stone & Masonry": "Stein und Mauerwerk",
        "Plastic & Rubber": "Kunststoff und Gummi", "Fabric & Leather": "Stoff und Leder",
        "Ceramic & Glass": "Keramik und Glas", "Organic": "Organisch",
        "Basic": "Basis", "Wear and grunge": "Verschleiß und Schmutz", "Details": "Details",
        "Hard surface": "Harte Oberfläche", "Stencil letters": "Schablonenbuchstaben",
        "Studio Soft": "Studio weich", "Studio Contrast": "Studio Kontrast",
        "Overcast": "Bewölkt", "Sunset": "Sonnenuntergang", "Warehouse": "Lager", "Night": "Nacht",
    },
    "es": {
        "Metal": "Metal", "Wood": "Madera", "Stone & Masonry": "Piedra y mampostería",
        "Plastic & Rubber": "Plástico y goma", "Fabric & Leather": "Tela y cuero",
        "Ceramic & Glass": "Cerámica y vidrio", "Organic": "Orgánico",
        "Basic": "Básico", "Wear and grunge": "Desgaste y suciedad", "Details": "Detalles",
        "Hard surface": "Superficie dura", "Stencil letters": "Letras de estarcido",
        "Studio Soft": "Estudio suave", "Studio Contrast": "Estudio contraste",
        "Overcast": "Nublado", "Sunset": "Atardecer", "Warehouse": "Almacén", "Night": "Noche",
    },
    "it": {
        "Metal": "Metallo", "Wood": "Legno", "Stone & Masonry": "Pietra e muratura",
        "Plastic & Rubber": "Plastica e gomma", "Fabric & Leather": "Tessuto e cuoio",
        "Ceramic & Glass": "Ceramica e vetro", "Organic": "Organico",
        "Basic": "Base", "Wear and grunge": "Usura e sporco", "Details": "Dettagli",
        "Hard surface": "Superficie dura", "Stencil letters": "Lettere mascherate",
        "Studio Soft": "Studio morbido", "Studio Contrast": "Studio contrasto",
        "Overcast": "Coperto", "Sunset": "Tramonto", "Warehouse": "Magazzino", "Night": "Notte",
    },
    "pt-BR": {
        "Metal": "Metal", "Wood": "Madeira", "Stone & Masonry": "Pedra e alvenaria",
        "Plastic & Rubber": "Plástico e borracha", "Fabric & Leather": "Tecido e couro",
        "Ceramic & Glass": "Cerâmica e vidro", "Organic": "Orgânico",
        "Basic": "Básico", "Wear and grunge": "Desgaste e sujeira", "Details": "Detalhes",
        "Hard surface": "Superfície dura", "Stencil letters": "Letras de estêncil",
        "Studio Soft": "Estúdio suave", "Studio Contrast": "Estúdio contraste",
        "Overcast": "Nublado", "Sunset": "Pôr do sol", "Warehouse": "Armazém", "Night": "Noite",
    },
    "ja": {
        "Metal": "金属", "Wood": "木材", "Stone & Masonry": "石と石造り",
        "Plastic & Rubber": "プラスチックとゴム", "Fabric & Leather": "布と革",
        "Ceramic & Glass": "陶器とガラス", "Organic": "有機物",
        "Basic": "基本", "Wear and grunge": "摩耗と汚れ", "Details": "ディテール",
        "Hard surface": "硬質表面", "Stencil letters": "ステンシル文字",
        "Studio Soft": "スタジオ ソフト", "Studio Contrast": "スタジオ コントラスト",
        "Overcast": "曇り", "Sunset": "夕暮れ", "Warehouse": "倉庫", "Night": "夜",
    },
    "ko": {
        "Metal": "금속", "Wood": "나무", "Stone & Masonry": "석재 및 석공",
        "Plastic & Rubber": "플라스틱 및 고무", "Fabric & Leather": "직물 및 가죽",
        "Ceramic & Glass": "도자기 및 유리", "Organic": "유기물",
        "Basic": "기본", "Wear and grunge": "마모 및 얼룩", "Details": "세부",
        "Hard surface": "경질 표면", "Stencil letters": "스텐실 문자",
        "Studio Soft": "스튜디오 부드러운", "Studio Contrast": "스튜디오 대비",
        "Overcast": "흐림", "Sunset": "일몰", "Warehouse": "창고", "Night": "밤",
    },
    "ar": {
        "Metal": "معادن", "Wood": "خشب", "Stone & Masonry": "حجر وبلوكات",
        "Plastic & Rubber": "بلاستيك و مطاط", "Fabric & Leather": "قماش و جلد",
        "Ceramic & Glass": "سيراميك و زجاج", "Organic": "عضوي",
        "Basic": "أساسي", "Wear and grunge": "تآكل وأوساخ", "Details": "تفاصيل",
        "Hard surface": "سطح صلب", "Stencil letters": "حروف قوالب",
        "Studio Soft": "استوديو ناعم", "Studio Contrast": "استوديو تباين",
        "Overcast": "غائم", "Sunset": "غروب", "Warehouse": "مستودع", "Night": "ليل",
    },
}

def data_pairs(code: str):
    """(source, translation) rows for the data-driven labels in `code`.

    Only rows whose translation differs from the English source are emitted, so a
    locale that lacks a given name contributes no row and tr() falls back to it.
    """
    table = OTHER.get(code, ZH if code == "zh-Hans" else {})
    return [(src, table[src]) for src in table if table.get(src) and table[src] != src]

def main():
    os.makedirs(LOCALES_DIR, exist_ok=True)
    total = 0
    for filename, code in MODULES.items():
        rs_path = os.path.join(I18N_DIR, filename)
        if not os.path.exists(rs_path):
            print(f"  SKIP {filename} (not found at {rs_path})")
            continue
        with open(rs_path, 'r', encoding='utf-8') as f:
            text = f.read()
        pairs = extract_string_pairs(text)
        if not pairs:
            print(f"  WARN {filename}: 0 pairs parsed (file has {len(text)} chars)")
            continue
        # Append the data-driven labels (materials, masks, brushes, alphas,
        # environments) that live in the engine crates, not the UI TABLEs.
        # craft-i18n keys on the English source text, so a name that also appears
        # in the UI TABLE (e.g. "Square", "Grunge", "Star") is a duplicate key;
        # dedupe keeping the UI entry first (tr() looks up by source anyway).
        data = data_pairs(code)
        seen = set()
        all_pairs = []
        for src, tr in list(pairs) + data:
            key = src.casefold()
            if key in seen:
                continue
            seen.add(key)
            all_pairs.append((src, tr))
        tsv_dir = os.path.join(LOCALES_DIR, code)
        os.makedirs(tsv_dir, exist_ok=True)
        tsv_path = os.path.join(tsv_dir, "texturecraft.tsv")
        with open(tsv_path, 'w', encoding='utf-8') as f:
            for src, tr in all_pairs:
                f.write(f"\t{escape_tsv(src)}\t{escape_tsv(tr)}\n")
        print(f"  {filename} -> {code}: {len(pairs)} UI + {len(data)} data entries")
        total += len(all_pairs)
    print(f"\nTotal: {total} entries in {len(MODULES)} locales")

if __name__ == "__main__":
    main()
