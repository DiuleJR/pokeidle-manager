# Política de segurança

Ainda não há um contato dedicado de segurança nem um canal de lançamento com suporte estabelecido. Não envie detalhes de vulnerabilidades para endereços encontrados fora deste repositório e não inclua segredos ou dados pessoais de contas em Issues ou logs públicos. Antes de distribuir binários com suporte oficial, as pessoas mantenedoras devem adicionar um canal privado de comunicação verificado.

Ao relatar um problema, informe a versão ou o commit afetado, o sistema operacional, os passos para reproduzi-lo e o impacto. Remova nomes de contas, cookies do navegador, tokens, conteúdo do banco de dados e detalhes de rede que possam identificar você. Dê tempo para que as pessoas mantenedoras investiguem e preparem uma correção antes da divulgação pública.

## Avisos importantes

- Os perfis do navegador são persistidos na pasta de dados do aplicativo e podem conter cookies de sessões autenticadas. Trate essa pasta como dado que contém credenciais.
- O servidor opcional do painel mobile é apenas para desenvolvimento e escuta no endereço local (`loopback`). Não o exponha à rede local nem à internet.
- A variável opcional `POKEIDLE_MOBILE_ZEROTIER_IP` ajusta hosts permitidos e a verificação de `Origin` do Vite, mas o servidor permanece vinculado a `127.0.0.1` no código deste repositório. A verificação de `Origin` não é autenticação; não a use como controle de acesso.
- Os diagnósticos do Rust são enviados a `stderr` pelo assinante do `tracing`. O sistema operacional ou um inicializador pode capturá-los. Há sanitização para alguns quadros do protocolo inspecionados, mas não presuma que os logs nunca contenham informações sensíveis.
