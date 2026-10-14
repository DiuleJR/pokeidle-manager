# Captura de protocolo para desenvolvimento

O Protocol Tap é um recurso local, opt-in e somente para builds de
desenvolvimento (`debug_assertions`). Ele observa as mesmas chamadas que
alimentam o Inspector, sem alterar, enviar ou responder frames. Builds release
não compilam o módulo nem criam arquivos de captura. Além disso, o Tap recusa
iniciar fora do app-data Smoke `com.pokeidle.manager.smoke`.

## Habilitação

Defina as variáveis apenas no processo de desenvolvimento:

```powershell
$env:POKEIDLE_DEV_PROTOCOL_TAP = '1'
$env:POKEIDLE_DEV_PROTOCOL_ACCOUNT = 'NicknameDaContaSmoke'
$env:POKEIDLE_DEV_PROTOCOL_FILTER = 'twitch,kick,live,stream,channel,viewer,online,offline,bonus'
$env:POKEIDLE_DEV_PROTOCOL_MODE = 'filtered_payload'
$env:POKEIDLE_DEV_PROTOCOL_RECORD = 'all_metadata'
npm run tauri:dev:smoke
```

O nome deve corresponder a exatamente uma conta salva no perfil Smoke, sem
diferenciar maiúsculas/minúsculas. O Manager resolve esse nome para o ID interno
persistente e usa o ID como filtro; o arquivo contém somente o apelido
sanitizado. Em uma execução instrumentada, o bootstrap e a recuperação
automáticos ficam restritos a essa conta. Os modos salvos das outras contas não
são alterados. Se o nome não corresponder a exatamente uma conta — ou qualquer
configuração do Tap falhar — nenhum bootstrap/recuperação automática de conta é
iniciado. Nessa situação, nenhum arquivo de captura é criado nem sessões antigas
são removidas. A conta-alvo pode ser adicionada pela UI; depois de salva com o
apelido correto, reinicie a Smoke com o Tap habilitado para capturar o próximo
welcome/reconnect. Sem o opt-in, o bootstrap normal permanece igual.

`MODE` aceita `metadata_only` ou `filtered_payload`. O segundo adiciona somente
uma projeção allowlist de `estado.twitch` e `estado.kick` em frames recebidos do
servidor. `RECORD` aceita `matching_only` (padrão) ou `all_metadata`; o filtro
procura termos configurados em chaves e strings, com limites de profundidade,
quantidade de nós e tamanho de frame. Mesmo em `all_metadata`, valores arbitrários
não são gravados. A projeção do payload só inclui `lives[]`/`oficiais[]` e os
campos explicitamente permitidos no código.

## Saída e limites

Os arquivos ficam no diretório de dados Tauri do aplicativo, em
`dev/protocol/protocol-tap-<epoch-ms>-<pid>.jsonl`. No Windows, o caminho é
derivado do diretório de dados do identificador `com.pokeidle.manager.smoke`;
consulte o aviso de inicialização do build DEV para o caminho exato. Acompanhe
localmente com `Get-Content -Wait <caminho>`.

Há um writer dedicado e canal limitado a 8 frames (até cerca de 4 MiB de cópias
temporárias). O hot path somente verifica o tamanho/conta, copia frames de até
512 KiB e tenta enfileirar; análise JSON e escrita ficam fora da thread do
WebSocket. Sob pressão, frames de diagnóstico são descartados e o próximo evento
registrado inclui `droppedBefore`. Cada arquivo tem teto de 10 MiB; ao iniciar,
somente arquivos `protocol-tap-*.jsonl` antigos nesta pasta específica são
limitados a duas sessões anteriores, mantendo no máximo três arquivos após a
criação da nova sessão. O writer faz flush por linha e fecha no encerramento
normal do Manager.

Nenhum endpoint externo é aberto. O tap nunca persiste frame bruto, credenciais,
tokens, cookies, cabeçalhos de autenticação ou IDs internos. A saída é uma projeção de diagnóstico;
ainda assim, os canais e contagens capturados devem permanecer locais e não devem
ser commitados ou enviados em issues. **Nunca adicione os JSONL de captura ao
Git**, mesmo quando contêm apenas projeções allowlist; eles podem revelar nomes
de canais e atividade. Apague os arquivos locais após concluir a investigação.
O Tap é exclusivamente diagnóstico de desenvolvimento, não é uma fonte de dados
nem uma dependência da página Lives em builds release.

## Formato

Cada linha registra timestamp, apelido da conta, direção, transporte, tipo de
frame sanitizado, tamanho original, chaves de topo permitidas pelo limitador,
caminhos que corresponderam ao filtro e, opcionalmente, a projeção allowlist.
Direções: `client_to_server` e `server_to_client`. Transportes: `browser` e
`background`.

O Inspector existente tem sanitização voltada à exibição e não é reutilizado
como exportador: a escrita do tap parte de uma allowlist independente. O limite
de execução é deliberadamente pequeno e limitado ao domínio de investigação.
