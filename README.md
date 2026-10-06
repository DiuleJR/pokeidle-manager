# Pokeidle Manager

O Pokeidle Manager é um aplicativo complementar não oficial para desktop, feito para organizar várias contas do Pokeidle e consultar informações de conta, hunts, inventário, automações e mercado. O projeto não é afiliado, endossado nem operado pelo jogo Pokeidle ou pelos detentores de seus direitos.

Este repositório é o código-fonte oficial da edição comunitária do projeto. Downloads oficiais e canais de suporte ainda não foram estabelecidos; compilações feitas a partir deste código são compilações comunitárias, não lançamentos oficiais do jogo.

## Compilar localmente

O aplicativo para desktop usa Tauri 2, Rust, Node.js e npm. No Windows, instale os pré-requisitos descritos no [guia oficial de pré-requisitos do Tauri v2](https://v2.tauri.app/start/prerequisites/) e execute:

```powershell
npm ci
npm run check
npm run build
npm run tauri:build:community
```

A edição comunitária é compilada sem um serviço comercial de licenciamento. O servidor opcional do painel mobile serve apenas para desenvolvimento; não o habilite em uma versão empacotada.

## Dados e rede

O aplicativo armazena o banco de dados SQLite, os recursos em cache e os perfis persistentes do navegador Brave de cada conta no diretório de dados do aplicativo definido pelo Tauri no sistema operacional. Os perfis do navegador podem conter cookies de login e outros dados de sessão. Remover uma conta do aplicativo não necessariamente remove seu perfil do navegador. Consulte [Privacidade](PRIVACY.md) para saber mais e ver os cuidados antes de apagar dados.

O aplicativo se conecta ao site e aos serviços do Pokeidle para obter dados do jogo e manter sessões, e pode buscar recursos durante a execução. As recomendações de hunt usam um script de referência comunitário obtido de `guiapokeidlehardtocapture.site`; o repositório aponta para a fonte, mas não redistribui esse script. Sprites do jogo e artes de terceiros sem autorização de redistribuição não são incluídos. Consulte [Ativos e ícones](docs/ASSETS.md).

## Contribuição e segurança

Leia [Como contribuir](CONTRIBUTING.md), o [Código de Conduta](CODE_OF_CONDUCT.md), a [Política de Segurança](SECURITY.md) e [Marcas e afiliação](TRADEMARKS.md). Os canais oficiais de contato e suporte ainda não foram estabelecidos; não presuma nem use canais não oficiais como se fossem autorizados pelo projeto.

## Licença

O código do projeto é disponibilizado sob a licença **GPL-3.0-only**; consulte [LICENSE](LICENSE). Componentes e dados de terceiros podem ter termos próprios. A revisão das licenças das dependências ainda não foi concluída; consulte [Avisos de terceiros](THIRD_PARTY_NOTICES.md). Este texto não constitui uma conclusão jurídica.
