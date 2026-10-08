# Estado incremental das contas

O desktop recebe atualizações frequentes pelo DTO `AccountLiveSnapshot`, que contém somente os
campos necessários à interface em tempo real e ao Pokémon ativo. O depósito, o inventário e as
opções de hunt não fazem parte desse payload.

Esses três domínios são consultados sob demanda pelos comandos `account_depot`,
`account_inventory` e `account_hunt_options`. Cada conta mantém uma revisão monotônica por domínio.
O frontend deduplica leituras simultâneas, conserva os dados quando a revisão não muda e descarta
respostas de outra conta ou de uma revisão antiga. A hidratação inicial usa `dashboard_live`; o
polling recorrente usa `integration_live_snapshot`. Os comandos legados de snapshot permanecem
disponíveis com o formato original.

O depósito só muda de revisão quando muda um campo exibido pelo domínio. Alterações frequentes de
HP/XP não invalidam o depósito. Inventário é revisto quando mudam itens ou bolas; opções de hunt,
quando o catálogo recebido do jogo muda.

O worker do Mercado usa uma projeção própria com estado de conexão, transporte, fuso do servidor e
saldos. Assim, consultas recorrentes do Mercado não precisam clonar os depósitos nem os catálogos
de hunt das contas.

## Medição validada

Na validação de referência com quatro contas durante 60 minutos, o payload recorrente caiu de
aproximadamente 3,36 MB para 11,5–11,8 KB (redução aproximada de 99,65%). O heap do WebView2 ficou
em torno de 9–10 MiB e sua memória privada normalmente em 236–242 MiB, sem crescimento progressivo
observado. Picos isolados recuaram nas amostras seguintes.

Esses números descrevem aquela execução de validação, não são limites rígidos de CI. O build,
coletores, timers e APIs de profiling usados para medir não fazem parte do produto normal.

## Compatibilidade

- A integração móvel continua usando suas projeções e comandos próprios.
- O caminho de conexão e troca entre Browser e Background não mudou nesta otimização.
- A rota de detalhes não baixa o depósito só para mostrar o Pokémon ativo; o resumo já contém essa
  informação. Se um encontro ativo exigir tipos do depósito, a leitura ocorre sob demanda.
- O retorno legado de `integration_snapshot` permanece disponível para consumidores existentes.
