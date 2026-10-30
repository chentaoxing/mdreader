# Fullwidth and Halfwidth Mixed Content

## Mixed Width Numbers

Halfwidth: 1234567890
Fullwidth: １２３４５６７８９０
Mixed: 12３45６７8９0

## Mixed Width Letters

Halfwidth: ABCDEFGHIJKLMNOPQRSTUVWXYZ
Fullwidth: ＡＢＣＤＥＦＧＨＩＪＫＬＭＮＯＰＱＲＳＴＵＶＷＸＹＺ
Mixed: ABＣDEＦGHIＪKLMＮOPQR

## Mixed Width Symbols

Halfwidth: @#$%^&*()
Fullwidth: ＠＃＄％＾＆＊（）
Mixed: @#＄％^&＊（）

## CJK with Latin

这是一个test，包含mixed content。

日本語のtestです。Englishも含まれています。

한국어 테스트입니다. English도 포함되어 있습니다.

## Halfwidth Katakana

ﾋﾞｯｸﾞｻｲｽﾞのｶﾀｶﾅ (halfwidth katakana)

ビッグサイズのカタカナ (fullwidth katakana)

## Width-Sensitive Alignment

| Halfwidth | Fullwidth | Mixed |
|-----------|-----------|-------|
| abc | ａｂｃ | aｂc |
| 123 | １２３ | 1２3 |
| ABC | ＡＢＣ | AＢC |

## Punctuation Mixing

English: Hello, world! How are you?
Chinese: 你好，世界！你怎么样？
Mixed: Hello你好，world世界！How怎么样you你？

## Width in Code

```
Halfwidth code: if (x > 0) return true;
Fullwidth code: ｉｆ（ｘ＞０）ｒｅｔｕｒｎ ｔｒｕｅ；
```

## Tricky Normalization Cases

- U+00B2 (²) vs U+0032 U+0302 (2̂) — different!
- U+00B9 (¹) vs U+0031 U+0301 (1́) — different!
- U+2126 (Ω) vs U+03A9 (Ω) — same meaning, different code points
- U+212A (K) vs U+004B (K) — Kelvin sign vs Latin K
- U+212B (Å) vs U+00C5 (Å) vs U+0041 U+030A (Å) — three representations
