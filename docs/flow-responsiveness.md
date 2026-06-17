# Responsividade entre Flow, bridge e interface

## Estado dos HARs

Os arquivos `imagens-flow.har` e `videos-flow.har` foram removidos durante a
limpeza de artefatos grandes e nao estavam disponiveis na Lixeira nem nas
localizacoes usuais pesquisadas.

As melhorias desta etapa foram baseadas no contrato observado que ja havia sido
incorporado em `extension/page_bridge.js` e `src-tauri/src/lib.rs`.

## Contrato observado

A geracao remota trabalha com identificadores distintos:

- `sourceOrder`: posicao narrativa local
- `mediaId`: identidade da imagem ou video no Flow
- `imageMediaId`: identidade da imagem base usada em animacao
- `workflowId`: workflow remoto relacionado
- `operationId`: operacao assincrona do video
- `batchId`: lote de submissao

Para videos, a submissao retorna os IDs rapidamente e o trabalho pesado continua
no servidor. A bridge consulta `batchCheckAsyncVideoGenerationStatus` ate o
resultado ficar pronto.

## Melhorias aplicadas

### Eventos por slot

A extensao agora publica:

- video agendado, assim que recebe os IDs
- mudanca de status remoto, somente quando o status muda
- IDs de midia, workflow, operacao e lote
- creditos restantes quando informados pelo Flow

### Persistencia e repasse

O backend:

- persiste os campos recebidos no slot correspondente
- repassa o payload completo no evento `flowcontent-slot-updated`
- emite outro evento quando a imagem intermediaria termina de ser salva

### Atualizacao da interface

O frontend:

- atualiza somente o slot identificado por `sourceOrder`
- mostra `mediaId`, `imageMediaId` e `operationId` na grade
- mantem o ID completo no tooltip
- usa leitura completa do projeto apenas para reconciliacao
- interrompe polling detalhado quando nao ha slots ativos

## Resultado esperado

O usuario passa a enxergar o processo remoto logo apos a submissao, mesmo antes
do arquivo final existir localmente. A interface deixa de depender exclusivamente
de polling e reduz leituras repetidas depois que a geracao termina.

## Proxima captura

Use:

```powershell
npm run dev:monitor
```

Essa captura sanitizada deve substituir o uso manual de HARs soltos. Depois da
sessao:

```powershell
npm run trace:analyze
```

O relatorio permite comparar rotas, transicoes de status, creditos e sinais de
rate limit sem manter HARs grandes na raiz do projeto.
