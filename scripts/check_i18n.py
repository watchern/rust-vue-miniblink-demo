"""i18n 文案表覆盖率自检。

两个语种的 key 集合必须完全一致：少一个 key 就等于那个语种下会出现裸 key
（i18n/index.ts 的 t() 会 fallback 到中文表，再 fallback 到 key 本身）。

用法：python scripts/check_i18n.py
退出码 0 = 通过，1 = 有缺失/多余
"""
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
I18N = ROOT / 'src' / 'i18n' / 'index.ts'
SRC = ROOT / 'src'


def parse_blocks(text: str) -> dict[str, list[str]]:
    """把 DICT 里每个语种块的 key 抠出来。

    块结构固定为：
      'zh-CN': {
        'key': '...',
      },
    用行首缩进 + 引号来判定块边界即可，不必引入 JS 解析器。
    """
    blocks: dict[str, list[str]] = {}
    cur: str | None = None
    for line in text.splitlines():
        m = re.match(r"\s*'([\w-]+)':\s*\{\s*$", line)
        if m:
            cur = m.group(1)
            blocks[cur] = []
            continue
        if cur and re.match(r"\s*\},?\s*$", line):
            cur = None
            continue
        if cur:
            km = re.match(r"\s*'([\w.]+)':", line)
            if km:
                blocks[cur].append(km.group(1))
    return blocks


def main() -> int:
    if not I18N.exists():
        print(f'找不到 {I18N}')
        return 1
    text = I18N.read_text(encoding='utf-8')
    blocks = parse_blocks(text)

    locales = list(blocks)
    print(f'文案表: {I18N.relative_to(ROOT)}')
    for lc in locales:
        print(f'  {lc:8s} {len(blocks[lc])} 个 key')

    ok = True
    if 'zh-CN' not in blocks or 'en-US' not in blocks:
        print('FAIL: 必须同时存在 zh-CN 与 en-US')
        return 1

    zh, en = set(blocks['zh-CN']), set(blocks['en-US'])
    # 同一语种内部不应有重复 key（后者会静默覆盖前者）
    for lc in locales:
        dup = {k for k in blocks[lc] if blocks[lc].count(k) > 1}
        if dup:
            ok = False
            print(f'FAIL: {lc} 内部重复 key: {sorted(dup)}')

    only_zh = sorted(zh - en)
    only_en = sorted(en - zh)
    if only_en:
        ok = False
        print(f'FAIL: en-US 缺少 {len(only_en)} 个 key:')
        for k in only_en:
            print(f'  - {k}')
    if only_zh:
        ok = False
        print(f'FAIL: zh-CN 缺少 {len(only_zh)} 个 key:')
        for k in only_zh:
            print(f'  - {k}')

    # 反向检查：源码里 t('...') 用到的 key 是否都存在
    used: set[str] = set()
    dynamic_tpl: set[str] = set()
    for f in list(SRC.rglob('*.vue')) + list(SRC.rglob('*.ts')):
        if f.name == 'index.ts' and f.parent.name == 'i18n':
            continue
        s = f.read_text(encoding='utf-8')
        used |= set(re.findall(r"\bt\(\s*'([\w.]+)'", s))
        # t(`nav.${it.id}`) —— 只记模板的前缀部分（nav./sub.），key 本身由 id 运行时拼出，
        # 静态核对不到完整集合，改为校验该前缀下确实存在 key。
        dynamic_tpl |= set(re.findall(r"\bt\(\s*`([\w.]+)\.\$\{", s))

    unknown = sorted(k for k in used if k not in zh)
    for pfx in sorted(dynamic_tpl):
        if not any(k.startswith(pfx + '.') for k in zh):
            unknown.append(pfx + '.*')
    if unknown:
        ok = False
        print(f'FAIL: 源码引用了文案表里没有的 key（{len(unknown)} 个）:')
        for k in unknown:
            print(f'  - {k}')

    print(f'\n源码引用: {len(used)} 个静态 key + 模板前缀 {sorted(dynamic_tpl)}')
    print('=' * 46)
    if ok:
        print('i18n 覆盖检查通过')
        return 0
    print('i18n 覆盖检查未通过')
    return 1


if __name__ == '__main__':
    sys.exit(main())
