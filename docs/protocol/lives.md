# Twitch, Kick e catálogo de lives

## Escopo e proveniência

Há duas fontes distintas: um recorte de `welcome.estado` fornecido pelo usuário
e capturas locais independentes do Protocol Tap na Smoke, em 2026-10-13. Os arquivos JSONL permanecem no app-data local e
não são versionados. A tabela de campos descreve somente valores efetivamente
observados; não garante que sejam obrigatórios nem define toda a semântica.

## Evidência disponível

| Afirmação | Estado |
| --- | --- |
| `welcome` server→client contém `estado.twitch` e `estado.kick`. | CONFIRMADO em capturas próprias no Browser e no Background. |
| `estado.twitch.lives[]` contém `login`, `nome` e `espectadores`; `oficiais[]` contém `login`, `nome` e `bonus`. | CONFIRMADO em capturas próprias e no recorte do usuário. |
| `estado.kick.oficiais[]` contém `slug`, `nome`, `aoVivo`, `espectadores` e `ativo`. | CONFIRMADO em capturas próprias e no recorte do usuário. |
| Frames `estado` posteriores atualizam Twitch e Kick por patches parciais, às vezes separados. | CONFIRMADO em capturas próprias no Browser e no Background. |
| A contagem `espectadores` muda entre patches. | CONFIRMADO nas amostras capturadas; não estabelece cadência nem disponibilidade contínua. |
| Twitch possui `assistindo`, `pctAtual`, `pct`, `emLives`, `assistindoEm`, `canal`, `aoVivo` e `extra`. | CONFIRMADO COMO CAMPOS PRESENTES NO RECORTE; semântica e escopo ainda não validados. |
| Kick possui `login`, `vinculado`, `bonus`, `pct`, `minutos`, `recompensas` e `streamer`. | CONFIRMADO COMO CAMPOS PRESENTES NO RECORTE; semântica e escopo ainda não validados. |
| As listas `oficiais` ou `lives` são globais, não por conta. | DESCONHECIDO; o Manager seleciona uma fonte por vez, mas isso não prova a semântica do servidor. |
| Links dos canais podem ser derivados com segurança. | IMPLEMENTADO localmente a partir de identificadores validados e hosts fixos; nenhuma URL de canal veio no frame. |
| Um frame cliente→servidor explicitamente identificável como pedido de lives foi capturado. | NÃO OBSERVADO nas janelas inspecionadas; não exclui comando genérico ou ação local. |
| Abrir os painéis dispara request próprio. | NÃO OBSERVADO; sem correlação temporal inequívoca entre clique e patches. |
| Twitch e Kick seguem cadência fixa ou gatilho conhecido. | DESCONHECIDO; intervalos observados são amostras, não contrato. |
| O comportamento dos dados difere entre Browser e Background. | DESCONHECIDO; ambos entregaram `welcome` e foram observados recebendo patches `estado`. |

## Campos vistos no recorte

Os tipos abaixo descrevem os valores efetivamente presentes no único recorte,
não um schema garantido nem campos obrigatórios. `login` da conta e valores de
bônus associados à conta foram omitidos/redigidos por privacidade.

| Caminho | Tipo observado | Escopo/semântica |
| --- | --- | --- |
| `twitch.login`, `twitch.canal` | string | DESCONHECIDO |
| `twitch.aoVivo`, `twitch.assistindo` | boolean | DESCONHECIDO |
| `twitch.emLives`, `twitch.pctAtual`, `twitch.pct`, `twitch.extra` | number | DESCONHECIDO |
| `twitch.assistindoEm` | string[] | DESCONHECIDO |
| `twitch.lives[]` | `{ login: string, nome: string, espectadores: number }` | DESCONHECIDO se catálogo global ou por conta |
| `twitch.oficiais[]` | `{ login: string, nome: string, bonus: boolean }` | DESCONHECIDO se catálogo global ou por conta |
| `kick.login` | string | DESCONHECIDO |
| `kick.vinculado` | boolean | DESCONHECIDO |
| `kick.bonus` | objeto com `xp`, `captura`, `shiny` numéricos | valores redigidos; semântica exata desconhecida |
| `kick.pct`, `kick.minutos` | number | DESCONHECIDO |
| `kick.recompensas[]` | `{ tipo: string, titulo: string, custo: number }` | DESCONHECIDO |
| `kick.oficiais[]` | `{ slug: string, nome: string, aoVivo: boolean, espectadores: number, ativo: boolean }` | DESCONHECIDO se catálogo global ou por conta |
| `kick.streamer` | null | Presente como `null` neste recorte; significado desconhecido |

## Exemplo sanitizado (amostra parcial do recorte do usuário)

Este exemplo é uma amostra parcial fiel dos campos relevantes, não um frame
completo; algumas entradas das listas foram omitidas. Logins vinculados à conta
e o timestamp de bônus foram omitidos. Nomes públicos de canais, títulos e
contagens mostrados foram mantidos como recebidos.

```json
{
  "twitch": {
    "aoVivo": true,
    "assistindo": true,
    "emLives": 2,
    "assistindoEm": ["canal-a", "canal-b"],
    "pctAtual": 17.5,
    "canal": "canal-oficial",
    "lives": [
      { "login": "canal-a", "nome": "Canal A", "espectadores": 1044 },
      { "login": "canal-b", "nome": "Canal B", "espectadores": 558 }
    ],
    "oficiais": [
      { "login": "canal-oficial", "nome": "Canal Oficial", "bonus": true }
    ],
    "pct": 15,
    "extra": 2.5
  },
  "kick": {
    "vinculado": true,
    "pct": 15,
    "minutos": 60,
    "recompensas": [
      { "tipo": "xp", "titulo": "Bônus +15% XP PokeIdle", "custo": 300 },
      { "tipo": "captura", "titulo": "Bônus de +15% Capture Boost", "custo": 500 },
      { "tipo": "shiny", "titulo": "Bônus de +15% Secret Lure", "custo": 1000 }
    ],
    "oficiais": [
      { "slug": "canal-b", "nome": "Canal B", "aoVivo": true, "espectadores": 1715, "ativo": true }
    ],
    "streamer": null
  }
}
```

## Capturas e lifecycle

Em 2026-10-13, o Tap registrou `hello` cliente→servidor e `welcome`
servidor→cliente nas conexões Browser e Background de uma conta Smoke. Ambos
os `welcome` continham Twitch/Kick. Frames `estado` posteriores atualizaram as
plataformas em patches parciais, observados nos dois transportes; os valores de
espectadores variaram. Isso confirma atualizações recebidas pelo WebSocket, mas
não identifica se o servidor as envia por timer, evento ou outro gatilho. Essa
captura é independente do recorte de `welcome.estado` fornecido pelo usuário
pelo usuário.

Uma captura adicional em `all_metadata` mostrou `hello` nas duas conexões e
requests `market.*` em ciclos de aproximadamente 30 segundos. Na janela ociosa
inicial, não foi observado frame cliente→servidor explicitamente relacionado a
Twitch/Kick. Os patches `estado` chegaram primeiro no Background (Twitch às
17:39:57 e 17:40:57 UTC; Kick às 17:40:07 e 17:41:21 UTC). Depois que o usuário
relatou abrir ambos os painéis na Smoke, a conexão Browser continuou recebendo
patches: Kick às 17:43:23, 17:44:07 e 17:45:22; Twitch às 17:43:57 e 17:44:57
UTC. Nenhum frame de saída com tipo claramente relacionado a Twitch/Kick
apareceu nessa janela; os frames de saída registrados incluíram `ping`,
`aviso.dispensar` e requests `market.*`. Como o instante exato dos cliques não
foi registrado e nenhum comando de saída os identifica, não é possível afirmar
que os patches foram respostas à abertura dos painéis. Os espectadores
variaram entre patches. Os intervalos observados não estabelecem timer ou
cadência contratual para Twitch nem Kick.

## Escopo conservador da V1

A página Lives usa somente os frames recebidos pelo jogo: sem polling, APIs
externas ou consulta aos sites das plataformas.

- O Manager apresenta uma projeção de uma única conta-fonte conectada por vez;
  não mescla listas de contas. A fonte atual permanece selecionada enquanto
  elegível (Online em Browser ou Background). Se ficar inelegível, escolhe a
  primeira conta elegível na ordem estável dos IDs internos, mesmo antes de ela
  receber seu primeiro estado de lives.
- Na troca para uma fonte sem cache, as listas ficam vazias e `updatedAt` fica
  nulo até chegar o próximo welcome/patch dessa conta. Sem qualquer fonte
  elegível, o último snapshot é mantido como `stale` enquanto a conta-fonte
  ainda existir; remover a conta remove sua projeção. `stale` indica
  conectividade, não prazo de expiração; não há TTL.
- `updatedAt` é o instante do último patch de lives aceito para a fonte, não uma
  medição independente por plataforma ou canal.
- A V1 exibe somente canais Kick com `aoVivo: true`. O campo `ativo` é preservado
  na projeção, mas sua semântica não foi comprovada e não deve ser tratado como
  sinônimo de ao vivo.
- A UI não exibe dados pessoais de vínculo, progresso ou bônus da conta nem
  recompensas Kick. Campos como `assistindo`, percentuais, `bonus` e
  `recompensas` presentes no recorte não definem esse tipo de informação para a
  página.
- Os links são montados localmente a partir de `login`/`slug` validados e hosts
  HTTPS fixos (`www.twitch.tv` e `kick.com`); URL recebida pelo protocolo não é
  usada como destino.

Operacionalmente, `twitch.lives` parece representar canais exibidos pelo jogo.
É uma inferência para organizar a interface, não confirmação de lista global,
exaustiva ou idêntica para todas as contas. Escopo, critério de inclusão e
cadência continuam desconhecidos.

Não foi observada transição online/offline de canal, nem comparado o estado
entre contas. Não criar UI que afirme escopo global, URL de canal ou ciclo
exato de atualização ainda não confirmado. Não adicionar polling nem fontes
externas para preencher essas lacunas.

## Captura necessária para confirmar

Para fechar a lacuna de causalidade, com uma conta de teste explicitamente
identificada e sem executar ações de jogo:

1. Capturar os caminhos relevantes de `welcome.estado.twitch` e
   `welcome.estado.kick`, mantendo apenas a projeção sanitizada dos campos.
2. Marcar o horário exato de cada clique e registrar frames client→server e
   server→client imediatamente antes e depois de abrir/fechar cada painel,
   separadamente.
3. Comparar mudanças de `estado` mesmo quando o tipo do frame for genérico.
4. Se a lista mudar naturalmente, correlacionar o delta com a UI do jogo.
5. Registrar a cadência observada sem esperar indefinidamente; sem transição
   real, marcar o lifecycle como desconhecido.

Até novas evidências, escopo global vs. por conta, transição online/offline,
request disparado pelos painéis e cadência permanecem **DESCONHECIDOS**. Não há
URL no frame; os links da V1 são derivados localmente. Não confundir o recorte
do primeiro recorte com a captura local da Smoke.
