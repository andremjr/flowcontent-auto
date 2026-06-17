# FlowContent Auto

Aplicativo desktop Tauri para organizar producoes vinculadas ao Google Flow,
processar narracoes e preservar a ordem narrativa dos assets.

## Desenvolvimento

Instale as dependencias:

```powershell
npm install
```

Execute o app desktop em modo dev:

```powershell
npm run dev:desktop
```

Execute o app desktop com monitoramento e captura de diagnosticos:

```powershell
npm run dev:monitor
```

Execute apenas o frontend no navegador:

```powershell
npm run dev
```

O bundling de release esta desativado em `src-tauri/tauri.conf.json` e nao existe
script de build do executavel no `package.json`. O Tauri ainda compila um
binario debug temporario dentro de `src-tauri/target/` para executar o modo dev.

## Validacao

```powershell
npm run check
npm test
npm run test:srt
```

Use `npm run build` somente para validar o bundle do frontend. Esse comando nao
gera executavel desktop.

## Monitoramento

`npm run dev:monitor` abre o aplicativo e registra em `captures/test-session-*`:

- stdout e stderr do Tauri, Vite e backend Rust
- eventos, erros e chamadas IPC do aplicativo
- screenshots, console e rede sanitizada da aba Flow
- manifesto da sessao

Pressione `Ctrl+C` ao terminar. Gere um resumo com:

```powershell
npm run test:report
```

As capturas sao artefatos temporarios e estao ignoradas pelo Git.

## Estrutura principal

- `src/`: interface React e controladores do frontend
- `src-tauri/`: backend desktop e bridge local
- `extension/`: extensao usada pela sessao autenticada do Flow
- `core/`: regras puras cobertas pelos testes Node
- `srt/`: transcricao e segmentacao de narracao
- `capcut/`: exportacao de draft
- `tools/`: monitoramento e diagnosticos de desenvolvimento
- `tests/`: testes das regras centrais e da extensao

O plano e o historico da refatoracao estao em
`docs/legacy-refactor-plan.md`.
