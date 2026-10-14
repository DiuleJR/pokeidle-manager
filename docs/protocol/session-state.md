# Estado de sessão e `welcome.estado`

## Frames e direção

| Elemento | Direção | Estado da evidência |
| --- | --- | --- |
| `hello` | Cliente → servidor | CONFIRMADO em captura Smoke no Browser e Background (2026-10-13). |
| `welcome` | Servidor → cliente | CONFIRMADO em captura Smoke no Browser e Background; inclui `estado.twitch` e `estado.kick`. |
| `estado` | Servidor → cliente | CONFIRMADO em capturas Smoke posteriores ao `welcome`; patches Twitch/Kick chegaram separados em Browser e Background. |

Na captura Smoke de uma conta de teste em 2026-10-13, `hello` precedeu `welcome` nos dois
transportes. Frames `estado` posteriores atualizaram Twitch/Kick em patches
parciais e houve mudança nas contagens de espectadores; a captura não determina
cadência nem o request que os provocou. No Background também chegaram `amigos`,
`campo.init`, `batalha` e `campo`; essa ordem inicial foi observada uma vez e
não é obrigatória. O material legado continua sendo apenas **LEGADO** fora do
que foi recapturado aqui.

## `welcome.estado`

O parser atual (`src-tauri/src/protocol.rs`, `RemoteState`) reconhece campos de
conta, progressão, inventário, Pokémon, hunt e bônus. Para evitar transformar
DTOs em evidência de rede, este documento descreve o que o Manager implementa,
não uma garantia de que todos os campos sempre estejam presentes no servidor.

| Caminho | Implementação atual | Estado da evidência |
| --- | --- | --- |
| `estado.level` | Nível do treinador; projetado como `trainerLevel`. | IMPLEMENTADO, NÃO CAPTURADO NESTA REVISÃO |
| `estado.twitch.assistindo` | Booleano opcional usado para derivar o bônus Twitch da conta. | IMPLEMENTADO; campo também presente no recorte de `welcome.estado` fornecido pelo usuário |
| `estado.twitch.pctAtual` | Percentual preferido pelo runtime quando `assistindo` é true. | IMPLEMENTADO; campo também presente no recorte fornecido |
| `estado.twitch.pct` | Fallback de percentual quando `pctAtual` não está presente. | IMPLEMENTADO; campo também presente no recorte fornecido |
| `estado.twitch.lives` / `estado.twitch.oficiais` | Arrays de canais; `lives` inclui espectadores e `oficiais` inclui `bonus`. | CONFIRMADO em captura Smoke e no recorte do usuário; escopo permanece desconhecido |
| `estado.kick.oficiais` | Array de canais com `slug`, `nome`, `aoVivo`, `espectadores`, `ativo`. | CONFIRMADO em captura Smoke e no recorte do usuário |
| `estado.kick` | O parser preserva somente `oficiais[]` para a projeção Lives; outros campos do recorte não fazem parte desse DTO. | Estrutura parcial CONFIRMADA; semântica dos demais campos permanece desconhecida |

Um `estado` parcial é aplicado pelo runtime como patch dos campos presentes. A
captura Smoke observou patches separados de Twitch e Kick em Browser e
Background; isso não confirma cadência nem regra de remoção/expiração de bônus.
A página Lives usa uma projeção allowlist da única conta-fonte conectada
selecionada pelo Manager. A fonte permanece estável enquanto elegível; em
failover o runtime escolhe outra fonte elegível, mesmo antes de ela receber
dados. Sem cache dessa fonte, a UI aguarda o primeiro estado; sem fonte
elegível, conserva o último snapshot como stale enquanto a conta existir.
Isso descreve a política local, não a semântica do servidor. Ver
[lives.md](./lives.md). As regras aplicadas pelo código aos campos Twitch
conhecidos podem ser vistas em `src-tauri/src/accounts.rs::apply_state`; elas
documentam o comportamento do Manager, não a semântica integral do servidor.

## Exemplo e limitações

Foi fornecido um recorte de estado de uma conta. Uma captura local
independente de outra conta confirmou o envelope/direção de `welcome` e patches
`estado`; detalhes e limitações estão em [lives.md](./lives.md). Fixtures JSON
dos testes continuam sintéticas e não devem ser tratadas como capturas do jogo.

## Notas sobre o material legado

- `welcome` é uma sincronização inicial observada nesta captura; a ordem dos
  frames subsequentes não é garantida.
- O mapa antigo classificava `evento`/`guild` como não interpretados. Isso está
  desatualizado em relação ao parser/runtime atuais, que tipam alguns campos de
  XP desses grupos.
- Os campos Twitch/Kick observados em wire estão documentados em
  `lives.md`; a estrutura ainda é parcial e não foi adicionada ao DTO.
- Nenhum material legado encontrado confirmou a estrutura `kick` ou uma lista
  global de canais.
