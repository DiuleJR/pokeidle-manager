# Privacidade e dados locais

O Pokeidle Manager foi projetado como um aplicativo local para desktop, mas se comunica com serviços remotos necessários às sessões do jogo e aos dados de recursos e recomendações. Este documento descreve o comportamento observável no código; não é uma garantia sobre serviços de terceiros ou sobre o sistema operacional.

## Dados armazenados neste dispositivo

O aplicativo usa o diretório de dados do aplicativo definido pelo Tauri no sistema operacional. Nele, mantém um banco SQLite (`pokeidle-manager.db`) com apelidos e configurações de exibição das contas, configurações de automação, resumos de atividade (incluindo derrotas, capturas, totais de XP e ouro e resumos de erros), estado das hunts e regras, observações e histórico do mercado. Também armazena recursos em cache.

Para sessões gerenciadas do navegador, o aplicativo cria perfis persistentes do Brave em uma pasta `profiles` dentro dos dados do aplicativo, separados por conta. Um perfil pode conter cookies, sessões autenticadas, estado de navegação e outros dados do navegador. Esses perfis são independentes dos registros de conta do banco de dados. Remover uma conta apaga os registros controlados pelo gerenciador, mas pode manter intencionalmente o perfil correspondente do navegador; apagar apenas o banco de dados também não apaga esses perfis.

## Rede e diagnósticos

O aplicativo se comunica com `pokeidle.io` para sessões do jogo, do navegador e de WebSocket, e resolve ou mantém em cache recursos do jogo obtidos de fontes remotas. As recomendações de hunt podem buscar um script de referência em `guiapokeidlehardtocapture.site`. Esses serviços podem receber as solicitações de rede inerentes a essas funções e têm suas próprias práticas de privacidade.

Nesta revisão do código-fonte, não foi identificado um sistema de análise ou telemetria controlado pelo aplicativo. Os diagnósticos do Rust são enviados para `stderr`; o sistema operacional ou um processo supervisor pode capturá-los. Alguns quadros do protocolo são sanitizados antes de serem registrados, mas revise os logs para remover identificadores de conta ou dados sensíveis antes de compartilhá-los.

## Painel de desenvolvimento opcional

O recurso `mobile-local-server` é apenas para depuração e escuta em `127.0.0.1:1421`; ele fornece dados locais do painel/API ao frontend de desenvolvimento. Um auxiliar de desenvolvimento separado pode vincular o Vite a uma interface ZeroTier selecionada. Isso torna o painel de desenvolvimento acessível a dispositivos que alcançam essa interface; a verificação de `Origin` não é autenticação. Essas ferramentas não se destinam à produção nem a redes não confiáveis.

## Como apagar os dados locais

Feche o aplicativo e os processos Brave gerenciados antes de fazer uma limpeza manual. Para remover o banco de dados do gerenciador e o cache, apague a pasta de dados do aplicativo Tauri identificada como `com.pokeidle.manager`, no local de dados de aplicativos do sistema operacional. Isso também remove os demais dados do aplicativo armazenados ali. Para apagar sessões salvas do navegador, remova separadamente a pasta `profiles`; isso desconecta esses perfis e apaga permanentemente os dados de navegação correspondentes. Faça uma cópia de segurança do que quiser guardar. O aplicativo ainda não oferece um fluxo verificado para apagar todos os dados, e a desinstalação pode não remover a pasta de dados.

## Atualizações deste aviso

Se o armazenamento ou o comportamento de rede mudar, atualize este aviso antes de publicar uma nova versão. Este texto não descreve as práticas independentes de dados do Pokeidle, Brave, ZeroTier ou de outros serviços. O auxiliar de desenvolvimento ZeroTier é opcional e não é necessário para compilar ou executar a edição comunitária empacotada.
