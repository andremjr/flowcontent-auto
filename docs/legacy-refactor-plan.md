# Refatoracao do app legado

## Objetivo

Refatorar o aplicativo antigo sem alterar a interface, os fluxos centrais ou as regras do backend.
O foco e reduzir o acoplamento entre:

- renderizacao da UI
- orquestracao de estado
- adaptadores desktop / Flow / arquivos locais

## Estado atual

Antes desta etapa, `src/App.tsx` concentrava:

- autenticacao e bootstrap do app
- polling de bridge, progresso e detalhes de projeto
- sincronizacao entre lista de projetos e projeto selecionado
- leitura de previews locais e remotos
- handlers de audio, prompts, geracao, retry, CapCut e AssemblyAI
- toda a arvore de renderizacao

Isso deixava a tela funcional, mas com muita logica espalhada num unico componente.

## Refatoracao aplicada agora

### 1. `src/App.tsx`

Passou a ser um componente de composicao:

- renderiza `AccessGate`
- monta a UI principal
- consome estado e handlers do hook `useAppController`

### 2. `src/hooks/useAppController.ts`

Novo controlador central do frontend legado. Ele concentra:

- estado de sessao e autenticacao
- estado de projetos e projeto selecionado
- polling do bridge e do progresso de geracao
- refresh do detalhe do projeto
- handlers de criacao, exclusao, prompts, audio, geracao e exportacao
- gerenciamento de previews locais e thumbnails de video

### 3. `src/lib/app-state.ts`

Novo modulo para regras puras e metadados compartilhados:

- `stageCopy`
- `sectionTitles`
- estados iniciais do bridge / AssemblyAI
- helpers puros de slots, prompts e status

## Beneficio imediato

- `App.tsx` deixa de ser o centro da regra de negocio
- a camada de dados fica testavel sem depender da arvore visual
- proximas extracoes podem ser feitas sem retrabalhar a UI

## Proximos cortes recomendados

### Curto prazo

- extrair `useBridgePolling`
- extrair `useProjectDetailRuntime`
- extrair `useAssetPreviewCache`

### Medio prazo

- criar um modulo `project-actions` para handlers de CRUD e prompts
- criar um modulo `generation-actions` para dispatch, retry e animacao
- normalizar os textos de toast num catalogo unico

### Longo prazo

- introduzir um store explicito por dominio (`session`, `projects`, `generation`, `bridge`)
- deixar `App.tsx` apenas como shell de layout
- cobrir o controlador com testes de fluxo

## Regra de seguranca desta refatoracao

Nao mudar:

- contrato Tauri existente
- integracao com a extensao
- logica de criacao e vinculacao de projeto Flow
- ordem dos fluxos da interface

Qualquer modularizacao nova deve preservar esses contratos.

## Limpeza do workspace

Foram removidos:

- prototipo HTML/JavaScript anterior ao frontend React
- componentes e tipos de cenas que nao tinham importadores
- scripts SRT substituidos por `flowcontent_srt.py` e `segmenter.py`
- tentativa abandonada de reescrita
- builds, executaveis, caches, capturas, logs e traces regeneraveis

Foram mantidos:

- `core/`, porque suas regras sao usadas pelos testes Node
- `extension/`, porque o backend prepara e carrega essa extensao
- `srt/flowcontent_srt.py` e `srt/segmenter.py`, usados no processamento real
- `capcut/export_draft.py`, usado pelo comando de exportacao
- `tools/`, usados no modo dev monitorado

O bundling de release esta desativado enquanto o foco for teste em modo dev.

## Responsividade da geracao

A comunicacao de geracao agora e orientada por slot:

- a extensao publica mudancas relevantes de status com `sourceOrder`, `mediaId`,
  `imageMediaId`, `workflowId`, `operationId` e `remainingCredits`
- o backend persiste esses campos e repassa o payload completo para o frontend
- o frontend atualiza imediatamente apenas o slot afetado
- uma leitura completa do projeto fica reservada para reconciliacao e conclusao
- o polling detalhado para quando nao existem slots em fila ou processamento

Na grade de assets, cada slot mostra os identificadores curtos de midia, imagem
base e operacao. O valor completo continua disponivel no tooltip.
