# Contrato observado do Google Flow

Este documento descreve comportamentos observados em traces autorizados. Ele
não define uma API pública e não deve ser usado para reproduzir diretamente
endpoints privados.

## Trace 001: imagem, animação e upscale

Data observada: 12 de junho de 2026.

Projeto:

```text
projectId: 5c0d359a-9cb3-45d0-991a-b43a75084e3f
title: Jun 12, 12:24 PM
tool: PINHOLE
```

### Criação do projeto

- A interface cria o projeto antes de navegar para sua URL.
- A resposta retorna `projectId` e `projectInfo.projectTitle`.
- A URL do projeto contém o mesmo `projectId`.

### Geração da imagem

Entrada observada:

- Uma solicitação no lote.
- Modelo interno observado: `NARWHAL`.
- Proporção: `IMAGE_ASPECT_RATIO_LANDSCAPE`.
- Prompt em português.

Saída observada:

- `mediaId`: `53c0bbf1-27a9-407f-8877-9c06d44af1b8`.
- `workflowId`: `f7eceb43-74d2-49b3-bca9-908a9accb481`.
- Dimensões: `1376 × 768`.
- Visibilidade: `PRIVATE`.
- O Flow também retornou uma versão em inglês do prompt.

Para o nosso adaptador, a imagem é considerada concluída somente quando um
`mediaId` e dimensões são observados.

### Animação da imagem

Entrada observada:

- O `mediaId` da imagem foi referenciado como `startImage.mediaId`.
- Modelo: `veo_3_1_i2v_lite_low_priority`.
- Modo: `VIDEO_GENERATION_MODE_IMAGE_TO_VIDEO`.
- Capacidade: `VIDEO_MODEL_CAPABILITY_START_IMAGE`.
- Proporção: `VIDEO_ASPECT_RATIO_LANDSCAPE`.
- Resolução inicial: `VIDEO_RESOLUTION_720P`.
- Duração: `8s`.
- Uma solicitação no lote.

Saída inicial observada:

- `mediaId`: `2502691e-b129-4aac-9f7e-a770df2cf7b5`.
- `workflowId`: `8b90f32b-29f1-405b-92bb-ef40946d38b4`.
- Estado inicial: `MEDIA_GENERATION_STATUS_SCHEDULED`.
- O mesmo `workflowId` relaciona o vídeo às etapas posteriores.

Transição observada:

```text
12:26:18  SCHEDULED
12:26:29  ACTIVE
12:26:39  ACTIVE
12:26:50  ACTIVE
12:27:00  ACTIVE
12:27:10  SUCCESSFUL
```

O executor deve manter o item em voo até observar `SUCCESSFUL`, falha ou
cancelamento. Receber a resposta inicial não significa conclusão.

### Upscale observado

Após a animação, foi observada uma etapa separada de upscale:

- Entrada: `mediaId` do vídeo concluído.
- Modelo: `veo_3_1_upsampler_1080p`.
- Modo: `VIDEO_GENERATION_MODE_VIDEO_TO_VIDEO`.
- Capacidade: `VIDEO_MODEL_CAPABILITY_UPSCALING`.
- Resolução: `VIDEO_RESOLUTION_1080P`.
- Identificador da operação: sufixo `_upsampled`.
- O `workflowId` permaneceu o mesmo da animação.

Transição observada:

```text
12:27:47  PENDING
12:27:58  PENDING
12:28:08  PENDING
12:28:19  PENDING
12:28:29  PENDING
12:28:40  SUCCESSFUL
```

O upscale deve ser representado como uma etapa própria, não como outra
submissão independente do prompt original.

### Créditos

Saldo observado antes, durante e depois do trace:

```text
total: 49.890
top-up: 25.000
subscription: 24.890
```

Não houve variação observada durante este trace. Isso não autoriza classificar
as operações como gratuitas: o custo continua sendo confirmado na interface e
no saldo observado para cada execução.

### Implicações para a fila

- A imagem, a animação e o upscale formam uma cadeia de dependências.
- Um item dependente não pode sair da fila sem o `mediaId` da etapa anterior.
- Uma etapa permanece em voo enquanto o estado for `SCHEDULED`, `PENDING` ou
  `ACTIVE`.
- Apenas estados finais liberam a capacidade ocupada.
- Polling observado não é permissão para aumentar sua frequência.
- Nenhum sinal de rate limit foi observado neste trace.
