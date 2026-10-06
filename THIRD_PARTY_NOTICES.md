# Avisos de terceiros

A auditoria de dependências considerou os 537 registros de pacotes do arquivo de dependências do Cargo (incluindo recursos opcionais) e os 293 registros do arquivo de dependências do npm (incluindo pacotes opcionais específicos de plataforma). Os manifestos e metadados de bloqueio identificam as licenças; não foi identificada dependência com incompatibilidade evidente com a licença **GPL-3.0-only** do projeto. Este é um inventário técnico, não aconselhamento jurídico nem garantia de que todas as obrigações de redistribuição foram cumpridas. Antes de distribuir um instalador, confira os avisos das dependências exatas incluídas no pacote e preserve todos os avisos exigidos.

O projeto é distribuído sob a licença **GPL-3.0-only**. O texto completo está em [`LICENSE`](LICENSE) e é incluído no aplicativo como `licenses/GPL-3.0-only.txt`.

O grafo de dependências Rust inclui dados de raízes de confiança `webpki-roots` sob a licença **CDLA-Permissive-2.0**. O texto do acordo está em [docs/licenses/CDLA-Permissive-2.0.txt](docs/licenses/CDLA-Permissive-2.0.txt) e é configurado como recurso do aplicativo em `licenses/CDLA-Permissive-2.0.txt`, para acompanhar a versão empacotada. Preserve esse recurso em qualquer distribuição que incorpore os dados de `webpki-roots`. A [página oficial da CDLA-Permissive-2.0](https://cdla.dev/permissive-2-0/) também está disponível.

O grafo de dependências de desenvolvimento do npm inclui os dados de compatibilidade de navegadores do `caniuse-lite`, identificados como **CC-BY-4.0**. Eles são usados pelas ferramentas de compilação, não pelo aplicativo em execução. O código-fonte do projeto não copia esse conjunto de dados para o aplicativo; ao usá-lo ou redistribuí-lo, mantenha a atribuição do pacote e siga os [termos da CC-BY-4.0](https://creativecommons.org/licenses/by/4.0/).

Este repositório não inclui artes nem sprites de jogos de terceiros com autorização de redistribuição verificada. A resolução de ativos durante a execução e os serviços remotos continuam sujeitos aos termos de seus titulares. Consulte [Ativos e ícones](docs/ASSETS.md).
