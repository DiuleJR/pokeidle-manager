# Janela desktop e escala da interface

O Pokeidle Manager prioriza o uso em computadores. A composição aprovada para desktop é o padrão em 100%; em monitores menores, a janela deve usar o zoom nativo do WebView antes de reorganizar os layouts. O CSS responsivo é uma proteção para conteúdo realmente estreito, não a estratégia principal para notebooks.

## Escala da interface

A página Configurações oferece `Automático`, `100%`, `90%`, `80%` e `75%`. Instalações novas e instalações existentes sem essa preferência usam `Automático`. A seleção é salva localmente na tabela chave/valor `app_settings` do SQLite, na chave `interface_scale`; não há migração nem armazenamento em servidor. Valores ausentes ou desconhecidos retornam com segurança a `auto`.

O comando desktop em Rust usa `WebviewWindow::set_zoom` somente no WebView `main`. Ele não acessa as janelas de conta do Brave, o jogo, o transporte do navegador nem páginas externas. A aplicação da escala é imediata e não recarrega o React. As escalas manuais permanecem fixas quando a janela muda de monitor. `Automático` calcula as dimensões lógicas a partir da área de trabalho física do monitor atual e do fator de DPI desse monitor:

O modo automático escolhe a maior escala cuja janela-base ampliada caiba na área lógica útil do monitor, reservando uma margem de 24 pixels lógicos em cada dimensão.

A janela-base aprovada em 100% mede 1440 × 938 pixels lógicos (tamanho interno inicial configurado). Em cada escala, o tamanho interno lógico recomendado é a base multiplicada pelo fator de zoom: 100% = 1440 × 938; 90% = 1296 × 844,2; 80% = 1152 × 750,4; e 75% = 1080 × 703,5. Escalas manuais mantêm o zoom solicitado e ajustam o tamanho da janela à área disponível quando necessário.

O modo automático converte a área útil do monitor de pixels físicos para unidades lógicas usando o fator de DPI, reserva a margem de 24 pixels lógicos e seleciona a maior escala que comporte a janela-base ampliada. Se nem 75% couber, ainda seleciona 75% e limita a janela ao espaço disponível. Exemplos: 1366 × 768 com DPI de 100% → 75%; 1920 × 1080 com DPI de 100% → 100%; 1920 × 1080 com DPI de 125% → 80%; 1920 × 1080 com DPI de 150% → 75%; 2560 × 1440 com DPI de 100% → 100%.

O tamanho da janela é definido pelas dimensões internas lógicas do Tauri. O tamanho externo e a posição externa físicos preservam o centro atual e limitam o posicionamento ao retângulo físico da área útil; isso considera as margens e sombras da moldura nativa informadas pelo sistema. O redimensionamento e o reposicionamento acontecem antes da mudança de zoom, para que a área final já corresponda à nova escala. Na inicialização, a janela permanece oculta até que as preferências sejam lidas e o tamanho/zoom salvos sejam aplicados, evitando uma troca visível de 100% para a escala escolhida. O modo automático recalcula ao receber eventos nativos de mudança de monitor ou de DPI, mas não em cada redimensionamento. O fator em cache evita reaplicar tamanho e zoom ao mover a janela dentro do mesmo monitor. Não há temporizador de consulta da escala. Um redimensionamento programático não altera a preferência selecionada.

As mudanças de escala e os eventos relevantes do monitor passam por um único worker do backend, alimentado por um canal `watch` de capacidade um. Uma transição já iniciada termina uma vez; enquanto isso, novas solicitações substituem o destino pendente, de modo que etapas intermediárias são ignoradas e somente a solicitação mais recente é aplicada em seguida. Nenhum mutex fica retido durante chamadas nativas de monitor/janela. Repetir a escala já aplicada não altera a janela. A preferência é persistida após 180 ms sem uma nova solicitação; o encerramento também grava a escolha mais recente mantida em memória. O evento de DPI usa o mesmo worker, sem redimensionamento independente.

## Limites da janela e proteção responsiva

A janela inicial de 1440 × 938 pixels lógicos é limitada à área útil e centralizada. No Windows, a área útil exclui a barra de tarefas. As dimensões mínimas são limitadas à área lógica disponível no monitor atual, e a janela é enquadrada novamente quando há mudança de DPI. Origens negativas de monitores são preservadas nos cálculos de posição; se um monitor desaparecer, a janela inicial não permanece maior que a área útil escolhida.

Atualmente, a aplicação não salva entre execuções a posição ou o tamanho ajustados manualmente. A inicialização parte do tamanho-base configurado, aplica a escala salva localmente enquanto a janela está oculta e limita/centraliza a janela no monitor atual. O redimensionamento manual durante uma sessão não altera a preferência de escala.

As reorganizações amplas anteriores para notebooks foram removidas: rearranjo de indicadores/cartões do Dashboard, empilhamento de plataformas em Lives, reorganização de colunas em Onde Caçar e layouts compactos amplos em Mercado/Automações/Configurações. As proteções CSS restantes são intencionalmente limitadas ao transbordamento local de conteúdo largo do Mercado e a uma alternativa extrema de largura reduzida para a estrutura desktop e o Dashboard. Diálogos, tabelas e componentes devem controlar o próprio transbordamento; não adicionar rolagem horizontal global no `body` nem observadores de redimensionamento por cartão.

Os seletores de pixel art mantêm o comportamento atual de `image-rendering`. Não aplicar renderização pixelada indiscriminadamente a fundos ilustrados ou outras imagens. Zoom e rasterização por DPI são conceitos distintos: inspecione os sprites em escala reduzida antes de alterar as regras de renderização dos ativos.

## Escopo da validação

Os testes automatizados cobrem os padrões da preferência, leitura e fallback da persistência local, serialização dos presets, cálculos dos quatro tamanhos-base, escolha automática por área útil/DPI, limites e preservação do centro. Ainda é necessária uma matriz visual manual completa de todas as rotas, escalas, resoluções e monitores, menus suspensos, dicas, diálogos e campos de entrada antes de uma versão final.
