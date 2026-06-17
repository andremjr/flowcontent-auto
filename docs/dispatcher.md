# Política de despacho

## Objetivo

Permitir que o usuário organize lotes grandes sem ultrapassar saldo, capacidade
dos modelos ou limites temporais impostos pelo Google Flow.

Ter muitos itens na fila não significa executar muitos itens simultaneamente.

## Portões independentes

Um item só pode sair da fila quando todos os portões estiverem abertos:

1. **Sessão:** aba Flow autenticada, conectada e na conta esperada.
2. **Capacidade:** recurso e modelo aparecem como disponíveis/saudáveis.
3. **Créditos:** custo atual foi observado e cabe no saldo não reservado.
4. **Throughput:** não existe cooldown, rate limit ou envio em andamento além
   da capacidade validada.
5. **Autorização:** o usuário autorizou aquele custo máximo e configuração.

Saldo suficiente nunca abre sozinho o portão de throughput.

## Política conservadora

- Uma conta começa com `maxInFlight = 1`.
- A fila pode conter qualquer quantidade de itens.
- Uma solicitação que produz múltiplas gerações reserva o custo de todas elas.
- O próximo envio espera confirmação de aceitação do anterior.
- Concorrência maior que 1 exige capacidade explicitamente validada.
- Não são feitos envios de teste apenas para descobrir o limite.
- Não existe rotação automática de contas para aumentar throughput.

## Sinais observados

O adaptador converte sinais do Flow em eventos internos:

```text
SESSION_READY
CAPABILITIES_OBSERVED
CREDITS_OBSERVED
SUBMISSION_ACCEPTED
GENERATION_RUNNING
GENERATION_COMPLETED
GENERATION_FAILED
RATE_LIMITED
MODEL_UNAVAILABLE
COMPOSER_DISABLED
```

Um `RATE_LIMITED` ou `COMPOSER_DISABLED` fecha imediatamente o portão:

```text
throughputGate = CLOSED
maxInFlight = 1
cooldownUntil = retryAfter observado, quando existir
```

Sem `retryAfter`, o item permanece pausado até o Flow mostrar novamente um
estado pronto. Não existe loop rápido, tentativa agressiva ou jitter usado para
forçar passagem.

## Pseudocódigo

```text
while queue has items:
  observe Flow session

  if session is not ready:
    pause queue
    continue

  if rate limit or cooldown is active:
    wait for an observed ready state
    continue

  if inFlight >= validatedMaxInFlight:
    observe current generations
    continue

  item = next authorized item
  cost = observe current Flow cost(item.configuration)

  if cost is unknown or cost > availableUnreservedCredits:
    block item
    continue

  reserve cost
  submit one item through the Flow interface

  if submission is accepted:
    mark item in flight
  else:
    release reservation
    pause or block according to the observed error
```

## Métricas de conformidade

- Quantidade em fila.
- Quantidade em voo por conta.
- Intervalo entre submissões aceitas.
- Tempo de cooldown observado.
- Rate limits recebidos.
- Créditos observados, reservados e efetivamente consumidos.
- Diferença entre custo estimado e custo confirmado.
- Mudanças de modelo feitas automaticamente pelo Flow.

Essas métricas servem para reduzir carga e explicar decisões ao usuário. Elas
não devem ser usadas para procurar formas de ultrapassar os limites.
