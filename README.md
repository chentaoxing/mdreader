# gh-pages 部署物（勿手改）

内容来自 main 分支 `shell/web/site/`（含 wasm-pack 产物 `pkg/`）。

重建：

```bash
wasm-pack build wasm --target web --release --out-dir ../site/pkg
```

页面完全离线、无外链、无埋点。`.nojekyll` 让 GitHub Pages 直接按原样发布
（wasm 二进制文件名以下划线开头会被 Jekyll 过滤，见 gh-pages 分支根的 `.nojekyll`）。
