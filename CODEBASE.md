# http11-rs

本リポジトリ固有の指示。

- **良い設計のためには破壊的変更を積極的に行うこと**
- RTSP/1.0 や RTSP/2.0 も利用できること
- `Request` / `Response` / `RequestHead` / `ResponseHead` 間で API の一貫性を保つこと

## RFC について

- RFC 準拠を最優先すること
  - RFC 7230 は廃止されて RFC 9112 になってる
  - RFC 7231 は廃止されて RFC 9110 になってる
- RFC を確認する際は refs/ 以下を利用すること
- サンプルは RFC に準拠していること
- obs-text (0x80-FF) は opaque data として保持すること (RFC 9110 Section 5.5)
  - 対象は quoted-string / reason-phrase / field-value など obs-text を許容する全経路
  - 受信時は `char_indices()` ベースで走査し UTF-8 不変条件を保つこと (1 バイトずつ `as char` で String に push する経路は Latin-1 mojibake の原因になるため禁止)
  - char 単位走査では Unicode scalar `U+0080..=U+10FFFF` (surrogate 除く) まで opaque char として保持する (ABNF のオクテット表現を Unicode scalar に拡張解釈する)
  - CR / LF / NUL は引き続き reject する (RFC 9110 Section 5.5)
  - 送信側は US-ASCII を推奨、非 ASCII は RFC 8187 ext-value を使うこと

## Proxy について

- `examples/http11_reverse_proxy` を **お手本** とし、proxy 利用時の API 一貫性を常に意識すること

## サンプルについて

- サンプルは **お手本** なので性能と堅牢性を両立させること
