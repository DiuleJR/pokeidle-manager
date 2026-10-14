# Auditoria das melhorias da comunidade

## Mapa de dependências

```text
estado.level no welcome/estado
  -> AccountState / AccountLiveState
  -> AccountView.trainerLevel
  -> card Informações da conta

AccountLiveSnapshot.hunt_session
  -> accountFromLive / AccountView.huntSession
  -> Dashboard e detalhe individual
     (mesmos drops raros, contadores shiny e startedAtMs)

Market Reader (market.itens)
  -> MarketRuntime summaries em memória
  -> MarketSnapshot / useMarketStore
  -> editor e cards de compra automática

Browser/Background WebSocket recebido
  -> ServerFrame parser ou Unknown preservado
  -> projeção allowlist de lives
  -> snapshot compartilhado e página Lives
```

## Fatos confirmados no código

- Na auditoria inicial não existia diretório `docs/protocol/`. Ele foi recriado depois a partir de uma revisão sanitizada do material legado; veja [`protocol/README.md`](./protocol/README.md). O parser de [`src-tauri/src/protocol.rs`](../src-tauri/src/protocol.rs) reconhece os frames tipados atuais e conserva frames desconhecidos como `Unknown`; exemplos de testes não são evidência independente de captura real.
- O nível do treinador vem de `estado.level`, já projetado como `trainerLevel`. A causa do erro visual era a tela usar `account.level` nos dois cards; esse campo representa o Pokémon ativo quando presente.
- As estatísticas individuais e o snapshot do Dashboard compartilham `hunt_session`; a mudança visual não requer novo snapshot, consulta, timer ou persistência. O Dashboard já tinha um timer por segundo para a duração da hunt; ele pode atender também a duração da sessão.
- O Market Reader consulta uma única fonte compartilhada a cada 30 segundos. O Dashboard do Mercado lê snapshots locais a cada 10 segundos. A última observação global e o watchdog de 90 segundos já existem.
- O resumo por item era mantido mesmo quando um ID não aparecia em respostas posteriores. Antes desta correção não havia data por item nem distinção entre campo de preço ausente e preço explicitamente nulo; por isso ausência da linha não podia ser rotulada com segurança como “sem oferta”.
- `welcome.estado.twitch` contém somente estado de acompanhamento e percentual do bônus (`assistindo`, `pctAtual`, `pct`). Isso não confirma canais oficiais, viewers, status online, URL, lista de lives, Kick ou evento global.
- O Inspector observa apenas frames das conexões do jogo já controladas pelo Manager. Não é um capturador geral das conexões Twitch/Kick e não possui retenção de cinco minutos nem exportação automaticamente segura.

## Lives e evidência do protocolo

A aba Lives foi implementada nesta branch com a projeção allowlist dos campos
observados em `welcome.estado` e nos patches `estado` de Twitch/Kick. A página
não usa APIs externas, scraping nem consulta periódica às plataformas. O
formato, a semântica global ou por conta, a cadência e a causa dos patches
continuam limitados ao que está descrito como confirmado/inferido/desconhecido
em [`protocol/lives.md`](./protocol/lives.md). Não tratar os exemplos de teste
como evidência independente de rede.

## Revisão do material legado e informação adicional

O repositório antigo foi consultado somente para leitura. O README e a estrutura
geral de sessões são úteis como contexto histórico; afirmações de que `evento`,
`guild` e Twitch não estavam tipados no Manager ficaram desatualizadas frente ao
código desta branch. Nenhum dos documentos relevantes encontrados descreve um
catálogo global de lives Twitch/Kick ou seu ciclo de atualização. A versão
sanitizada e as classificações estão em [`protocol/session-state.md`](./protocol/session-state.md)
e [`protocol/lives.md`](./protocol/lives.md).

O usuário também informou que viu dados relacionados a `estado.twitch` e
`estado.kick` no `welcome`. Isso é registrado como relato manual, sem atribuir
schema aos campos e sem substituir a captura real exigida para confirmar tipos,
direção, conteúdo global/por conta e lifecycle.

## Limites desta implementação

As correções visuais do nível e do Dashboard reutilizam os DTOs existentes. O preço atual reaproveita o snapshot compartilhado do Market Reader e acrescenta apenas metadados pequenos por resumo de item: data da última observação e presença explícita dos campos `minGold`/`minOrb`. Isso permite mostrar preço, “Sem oferta” somente quando o campo está presente e nulo, “Sem dado”, “Ainda não carregado” e preços antigos como “Desatualizado”. Nenhum polling por item foi adicionado.

O status de preço “desatualizado” usa o mesmo limite de 90 segundos já empregado pelo watchdog do Reader. Como o snapshot do mercado já é atualizado pela aplicação, não foi criado timer adicional.

## Onde Caçar: elegibilidade e custo do cálculo

As estimativas consideram a interseção entre os slugs da referência e as
opções observadas para a conta selecionada quando essa interseção não está
vazia; caso contrário, usam as hunts da referência como alternativa. O limite
de nível é o nível do treinador da conta (`trainerLevel`), com o nível
observado da hunt — ou o nível da referência — comparado antes de simular
espécies, dano, tempo por abate, XP e ouro. A revisão da RC3 confirmou que as
hunts inelegíveis já eram filtradas antes da simulação; avaliar hunts fora do
nível não era a causa da demora.

A causa confirmada era a dependência da simulação em relação ao objeto inteiro
da conta: snapshots ao vivo chegam aproximadamente a cada 1,2 segundo e também
mudam campos que não participam do cálculo. Assim, dados irrelevantes podiam
disparar novamente a simulação completa. A assinatura agora inclui somente as
entradas que alteram a estimativa (treinador, Pokémon ativo, bônus e
opções/campos das hunts), evitando essa recomputação. A lista de regiões também
é indexada por slug uma vez por referência, em vez de procurar a hunt novamente
para cada resultado. Busca, filtros visuais e ordenação operam sobre as
estimativas calculadas. Os bônus de XP são aplicados às estimativas do
treinador e do Pokémon; não alteram dano, tempo de abate ou ouro.
