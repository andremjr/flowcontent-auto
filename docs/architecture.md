# Arquitetura inicial do FlowContent Auto

## Experiência pretendida

1. O usuário inicia o FlowContent Auto.
2. O aplicativo abre ou localiza uma janela Chromium dedicada ao Flow.
3. O usuário faz login manualmente na própria conta Google.
4. A extensão confirma que a sessão está pronta, sem exportar credenciais.
5. O usuário pode minimizar o navegador e trabalhar pelo FlowContent Auto.
6. A extensão executa e observa as ações dentro da aba autenticada.

## Princípio central

O FlowContent Auto é um plano de controle. O Google Flow continua sendo o
plano de execução, autenticado e operado no navegador do próprio usuário.

Nenhuma ação deve ser despachada quando saldo, custo, recurso disponível,
conexão da extensão ou autorização do usuário não estiverem confirmados.

## Fronteiras

### Aplicação

- Organiza projetos, lotes e prompts.
- Calcula uma reserva máxima, sem tratar a estimativa como saldo real.
- Pede à extensão uma leitura observável do estado da aba Flow.
- Exige autorização explícita antes de liberar um lote.
- Mantém log de intenção, autorização, despacho e resultado observado.

### Extensão

- Executa somente em origens do Google Flow declaradas no manifesto.
- Nunca recebe senha, cookie ou token de sessão da conta Google.
- Confirma que a aba ativa é o Flow e que o usuário está autenticado.
- Expõe capacidades observadas, não capacidades presumidas.
- Recebe comandos do aplicativo por um canal local autenticado.
- Opera a interface fornecida pelo Flow mesmo com a janela minimizada.
- Observa DOM e rede para correlacionar comandos, estados e resultados.
- Rejeita comandos desconhecidos, expirados ou incompatíveis.
- Retorna recibo de execução para cada comando.

### Google Flow

- É a fonte de verdade para saldo, custo atual, disponibilidade de modelo,
  permissões do plano e resultado da geração.
- Sua interface pode mudar; qualquer divergência interrompe o despacho.

## Estratégia de integração

### Descoberta e testes

Durante o desenvolvimento, registramos traces de ações manuais autorizadas:

- Screenshots e snapshots do DOM.
- Cliques, mudanças de seleção, navegação e estados visíveis.
- Requisições XHR/Fetch com método, rota, status e estrutura sanitizada.
- Respostas JSON sanitizadas usadas para reconhecer estados e resultados.

Os traces não devem conter cookies, cabeçalhos de autenticação, tokens,
senhas, segredos ou conteúdo sensível desnecessário.

### Execução produtiva

A extensão deve reproduzir o fluxo pela interface fornecida pelo Google Flow:

1. Abrir ou localizar a aba Flow autenticada.
2. Confirmar conta, capacidade, modelo e custo observáveis.
3. Preencher configurações e prompt.
4. Acionar a geração pela interface.
5. Observar aceitação, rate limit, falha, progresso e conclusão.
6. Retornar um recibo ao aplicativo.

As requisições capturadas ajudam a reconhecer eventos e validar o adaptador,
mas não são tratadas como uma API pública nem reproduzidas diretamente como
substituto da interface. Isso evita depender de tokens, endpoints privados ou
comportamentos que possam ignorar controles atuais do Flow.

### Canal local

Para produção, o canal preferido é Native Messaging entre a extensão e o
aplicativo instalado. Durante desenvolvimento, um WebSocket limitado a
`127.0.0.1`, autenticado por desafio efêmero, é suficiente.

O canal aceita comandos de alto nível, por exemplo:

- `OBSERVE_SESSION`
- `OBSERVE_CAPABILITIES`
- `CREATE_PROJECT`
- `CONFIGURE_GENERATION`
- `SUBMIT_GENERATION`
- `OBSERVE_GENERATION`
- `DOWNLOAD_RESULT`

Ele não aceita comandos arbitrários de JavaScript, URLs externas ou acesso a
credenciais da sessão.

## Máquina de estados de um item

```text
DRAFT
  -> PREFLIGHT_REQUIRED
  -> READY
  -> AUTHORIZED
  -> DISPATCHED
  -> OBSERVED_RUNNING
  -> COMPLETED | FAILED | CANCELLED

Qualquer divergência:
  -> BLOCKED
```

`AUTHORIZED` deve carregar uma validade curta e estar associado a:

- Conta observada.
- Origem e aba Flow observadas.
- Modelo, duração, quantidade de gerações e custo máximo.
- Identificador imutável do prompt.
- Ação explícita do usuário.

## Contrato mínimo de comando

```json
{
  "version": 1,
  "commandId": "cmd_01J...",
  "expiresAt": "2026-06-12T16:05:00Z",
  "flowTabId": 123,
  "observedAccountHash": "sha256:...",
  "action": "SUBMIT_GENERATION",
  "payload": {
    "promptId": "prompt_01J...",
    "prompt": "texto do usuário",
    "model": "observed-model-id",
    "durationSeconds": 8,
    "generationCount": 1
  },
  "authorization": {
    "maxCredits": 25,
    "authorizedAt": "2026-06-12T16:00:00Z",
    "authorizedBy": "local-user"
  }
}
```

## Regras que falham fechado

1. Não despachar quando o custo atual não puder ser observado.
2. Não somar créditos de contas diferentes em um único saldo.
3. Não alternar contas automaticamente para contornar limites.
4. Não repetir uma geração cobrada sem nova autorização.
5. Não inferir sucesso apenas pelo clique; aguardar estado observável no Flow.
6. Não continuar quando a interface observada divergir do adaptador conhecido.
7. Não comprar créditos ou habilitar recarga automática.
8. Não executar JavaScript arbitrário recebido do aplicativo.
9. Não reproduzir chamadas privadas como substituto da interface fornecida.
10. Não interpretar saldo disponível como permissão para paralelismo.
11. Não aumentar concorrência por tentativa e erro contra rate limits.
12. Não repetir automaticamente um item após resposta de rate limit.

A política completa de fila e throughput está em `docs/dispatcher.md`.
Os contratos descobertos em traces autorizados estão em
`docs/observed-flow-contract.md`.
O vínculo local/remoto e a preservação da ordem narrativa estão definidos em
`docs/data-model.md`.

## Roteiro de captura de traces

Cada cenário deve começar em um projeto vazio e terminar após o estado final:

1. Ler conta, plano, saldo e capacidades disponíveis.
2. Criar, renomear, abrir e excluir um projeto de teste.
3. Criar imagem com cada combinação relevante de modelo e proporção.
4. Criar vídeo com cada modelo, duração e quantidade suportados.
5. Enviar uma solicitação que gere múltiplos resultados.
6. Observar fila, execução, conclusão e falha sem cobrança.
7. Observar saldo antes e depois de uma geração cobrada.
8. Observar rate limit e recurso indisponível, sem tentar contorná-los.
9. Baixar um resultado pela ação disponível na interface.

Cada trace deve registrar a ação do usuário, o estado anterior, o custo
confirmado, o estado posterior e o recibo produzido pela extensão.

## Observações atuais sobre créditos

O Google informa que custos são por geração, não por solicitação, e que uma
solicitação pode criar múltiplas gerações. Também informa que limites e custos
podem mudar; portanto, a aplicação não deve manter uma tabela fixa como fonte
de verdade. A tabela local serve apenas para estimativa, e o valor exibido no
Flow precisa ser confirmado no preflight.

Fontes oficiais consultadas em 12 de junho de 2026:

- https://support.google.com/flow/answer/16526234
- https://one.google.com/about/google-ai-plans/
- https://policies.google.com/terms

## Próximos módulos

- Persistência local de projetos, prompts e recibos.
- Protocolo autenticado entre aplicação e extensão.
- Adaptador de leitura do Flow com testes de contrato.
- Tela de contas que mostra cada saldo separadamente.
- Importação/exportação de prompts e resultados.
