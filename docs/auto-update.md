# Auto-update

## Objetivo

Entregar uma versao do FlowContent Auto que:

- verifica se existe update publicado;
- baixa o instalador novo;
- fecha o app e conclui a atualizacao no Windows;
- evita reinstalacao manual a cada correcao.

## Arquivos principais

- `src-tauri/resources/update-config.json`: liga/desliga o updater e aponta para o `latest.json` publico.
- `tools/generate-updater-manifest.mjs`: gera o `latest.json` a partir do instalador NSIS e do `.sig`.

## Preparacao inicial

1. Edite `src-tauri/resources/update-config.json`.
2. Se usar outro repositorio no futuro, troque a URL do release endpoint.

Exemplo:

```json
{
  "enabled": true,
  "endpoints": [
    "https://github.com/andremjr/flowcontent-auto/releases/latest/download/latest.json"
  ]
}
```

## Build assinado

No PowerShell:

```powershell
$env:TAURI_SIGNING_PRIVATE_KEY_PATH = ".tauri/flowcontent-updater-protected.key"
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = "flowcontent-updater-2026"
npm run tauri build
```

Isso gera:

- instalador `.exe`
- instalador `.msi`
- arquivos `.sig` para updater

## Manifesto latest.json

Depois do build:

```powershell
$env:UPDATE_BASE_URL = "https://github.com/andremjr/flowcontent-auto/releases/download/v0.1.2"
$env:UPDATE_ASSET_NAME = "flowcontent_auto_0.1.2_x64_setup.exe"
npm run release:manifest
```

O script grava `latest.json` em `src-tauri/target/release/bundle/nsis/latest.json`.

## Upload para GitHub Releases

Publique na mesma release:

- `flowcontent_auto_0.1.2_x64_setup.exe`
- `flowcontent_auto_0.1.2_x64_setup.exe.sig`
- `latest.json`

## GitHub Actions

Para automatizar releases no GitHub, crie estes secrets no repositorio:

- `TAURI_SIGNING_PRIVATE_KEY`: conteudo completo de `.tauri/flowcontent-updater-protected.key`
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`: `flowcontent-updater-2026`

Depois, publique uma tag como `v0.1.2`. O workflow gera:

- instalador `.exe`
- assinatura `.sig`
- `latest.json`

e anexa tudo na release do GitHub.

## Fluxo para os alunos

1. Na primeira distribuicao com updater, eles ainda instalam manualmente a nova versao.
2. Nas proximas correcoes, o app detecta a atualizacao.
3. O aluno clica em `Atualizar` dentro do app.
4. O Windows fecha o app, instala a nova versao e reabre.
