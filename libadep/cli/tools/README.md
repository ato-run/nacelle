# ADEP CLI Tools

ADEP manifest 検証など、CLI 補助スクリプトを格納しています。

## Manifest Validator

```bash
node gumball-adep/cli/tools/validator-cli/validate.js bestyai-suite/apps/web/capsule/manifest.webcapsule.json
```

- `.webcapsule` / `.gwp` manifest のスキーマ・署名・SBOM・loopback 制約をチェックします。
- Webcapsule CLI やサンプルの CI からも利用できます。
