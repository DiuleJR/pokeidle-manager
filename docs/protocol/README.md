# Protocolo WebSocket

Esta pasta é a referência do protocolo usada pelo Manager. Cada afirmação deve
separar evidência de frame real, comportamento observado na interface e detalhes
que existem somente na implementação atual.

## Níveis de certeza

- **CONFIRMADO**: observado em frame real sanitizado, com direção e contexto
  registrados. Um tipo/DTO no código, sozinho, não confirma o wire format.
- **INFERIDO**: interpretação plausível de um frame ou comportamento, ainda sem
  correlação suficiente para afirmar sem ressalvas.
- **DESCONHECIDO**: não há evidência suficiente. Não implementar um schema ou
  comportamento presumido como se fosse fato.
- **IMPLEMENTADO, NÃO CAPTURADO NESTA REVISÃO**: o parser atual conhece o campo,
  mas não foi comparado a uma captura real desta revisão.

## Sanitização

Exemplos devem conter apenas os campos mínimos necessários. Remover tokens,
cookies, cabeçalhos de autorização, identificadores de dispositivo, IDs/nicks
pessoais, dados de conta, saldos, inventário e qualquer outro dado privado que
não seja necessário para explicar o protocolo. Não versionar logs de sessão ou
payloads autenticados integrais.

Não inventar frames nem preencher campos ausentes com exemplos hipotéticos.
Quando não houver uma captura real segura, registrar explicitamente que o
exemplo está ausente.

## Documentos

- [Estado de sessão e `welcome.estado`](./session-state.md)
- [Twitch/Kick e ciclo de atualização de lives](./lives.md)
- [Captura de protocolo para desenvolvimento](./dev-protocol-tap.md)

## Proveniência da revisão

Documentos legados foram consultados somente para leitura e serviram como
contexto histórico, não como autoridade automática: afirmações foram comparadas
com o parser e o runtime da edição comunitária. Nenhum conteúdo comercial,
chave, segredo, banco, cookie ou log autenticado foi trazido para este
repositório.
