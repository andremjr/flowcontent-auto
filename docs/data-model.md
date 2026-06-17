# Modelo de dados do projeto

## Fluxo principal

```text
Produção criada
  -> áudio enviado
  -> transcrição única
  -> SRT de legendas + SRT de assets
  -> prompts criados fora do aplicativo
  -> lista de prompts importada em ordem estrita
  -> assets gerados no Flow
  -> downloads e montagem
```

### Segmentação de assets pelo ritmo

O SRT de assets usa pausas reais da narração, não pontuação como regra
principal. Uma unidade entre pausas recebe `ceil(duração / 8s)` assets; uma
unidade com exatamente 8 segundos ainda cabe em um asset.

Quando uma unidade longa precisa de vários assets, cada bloco mantém:

- `focusText`: palavras faladas naquele intervalo.
- `contextText`: texto completo da unidade entre pausas.
- `pauseGroupId`: unidade narrativa compartilhada.
- `partIndex` e `partCount`: posição dentro da unidade.

O SRT de assets inclui foco e contexto quando uma unidade é dividida. Dessa
forma, uma expressão curta ou intercalada nunca é enviada isoladamente para o
gerador externo sem seu contexto narrativo.

### Cobertura das pausas

O tempo falado e o tempo visual são armazenados separadamente. O tempo visual
cobre também os silêncios entre frases, evitando quadros pretos.

Modos de transição:

- `midpoint` (padrão): troca no meio da pausa.
- `next-speech`: o asset anterior cobre toda a pausa e a troca ocorre quando a
  próxima fala começa.
- `previous-speech`: o próximo asset começa quando a fala anterior termina e
  cobre toda a pausa.

Uma pausa principal é, por padrão, um intervalo de pelo menos `600 ms` entre
duas palavras. O limiar é configurável por produção. Pausas menores continuam
sendo usadas para escolher cortes internos naturais.

## Regra principal

O núcleo do FlowContent Auto é um storyboard ordenado. A ordem narrativa não
pode depender da ordem atual de execução, do sucesso de uma geração ou da
existência de um arquivo baixado.

Na interface, esse storyboard aparece como **Mapa de Cenas**.

## Identidades

### Projeto

- `localProjectId`: identidade do workspace local.
- `flowProjectId`: projeto remoto associado no Google Flow.
- `accountHash`: conta Flow esperada, sem armazenar e-mail ou credencial.

### Slot narrativo

- `slotId`: identidade permanente de uma posição narrativa.
- `sceneCode`: identificador humano estável, como `scene-0012`.
- `ordinal`: ordem imutável recebida na lista de prompts.
- `prompt`: intenção original daquela posição.

Uma falha em `scene-0012` não move `scene-0013` para o lugar dela.

Antes de existir um prompt, a posição narrativa nasce como `assetBlockId` a
partir do SRT de assets. A lista importada precisa ter exatamente a mesma
quantidade de prompts; caso contrário, nenhuma associação é aplicada.

### Tentativa

- `attemptId`: cada geração ou retry dentro de um slot.
- `attemptNumber`: sequência de tentativas daquele slot.
- `state`: estado observado da execução.
- `workflowId`: workflow observado no Flow.
- `mediaId`: resultado remoto observado no Flow.
- `download.relativePath`: arquivo local opcional.

Um slot pode ter várias variantes bem-sucedidas. `activeAttemptId` indica qual
delas está escolhida para edição e sincronização.

## Arquivos locais

```text
Projeto Aurora/
  .flowcontent/
    project.json
    storyboard.json
    timeline.json
  prompts/
  audio/
  downloads/
    scene-0001/
    scene-0002/
```

`storyboard.json` mantém ordem, tentativas e IDs remotos. Ele não precisa
armazenar a mídia remota.

`timeline.json` referencia o SRT e associa cues a `slotId` + `attemptId`.
Também registra `flowMediaId` e o caminho local quando houver download.

O importador preserva a ordem original e o hash do SRT. Uma associação
sequencial automática é apenas uma sugestão: ela nunca confirma silenciosamente
um asset nem remove buracos existentes no storyboard.

## Reidratação

Ao abrir um projeto:

1. Ler `project.json` e validar a conta conectada.
2. Pedir à extensão `OBSERVE_PROJECT(flowProjectId)`.
3. Reconciliar mídias remotas por `mediaId` e `workflowId`.
4. Manter slots ausentes como buracos explícitos.
5. Marcar downloads locais ausentes sem remover a referência remota.

Mídias existentes no Flow sem vínculo conhecido aparecem como **assets remotos
não atribuídos**. Elas podem ser visualizadas e atribuídas manualmente a um
slot, mas nunca entram automaticamente no storyboard nem alteram sua ordem.

## Exclusões

- Excluir um download local não exclui o resultado no Flow.
- Desvincular um projeto local não exclui a pasta nem o projeto no Flow.
- Excluir um item no Flow é uma operação remota separada e explícita.
- Nenhuma exclusão reordena automaticamente o storyboard.
