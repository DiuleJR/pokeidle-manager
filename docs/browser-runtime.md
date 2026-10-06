# Runtime de navegador — auditoria e POC local

## Resultado em uma frase

A POC técnica foi positiva: a mesma árvore do Brave, copiada para fora de `Program Files`, iniciou pelo resolvedor do Manager, respondeu ao CDP, navegou e reabriu um perfil isolado mantendo `localStorage`. Isso prova portabilidade técnica neste Windows, não autorização para redistribuir o navegador. A análise de licença está separada em [browser-runtime-licensing.md](browser-runtime-licensing.md).

## Arquitetura atual

1. `WindowsBraveManager::detect_brave` resolve o executável. Antes desta POC, verificava `PROGRAMFILES`, `LOCALAPPDATA` e `PROGRAMFILES(X86)`, em `BraveSoftware/Brave-Browser/Application/brave.exe`.
2. `start_observer` recebe `app_data_dir` como base e calcula o perfil persistente em `profiles/<account_id>`. O perfil é por conta e fica separado do perfil pessoal do navegador.
3. Um porto local livre é reservado para CDP. O Manager inicia o processo com `--user-data-dir=<perfil>`, `--remote-debugging-address=127.0.0.1`, `--remote-debugging-port=<porto>` e `--remote-allow-origins=http://127.0.0.1`. `--no-first-run` e `--no-default-browser-check` evitam telas de primeiro uso e o prompt de navegador padrão.
4. O modo interativo pode abrir `--new-window` com a URL inicial. Os modos com navegação controlada iniciam sem URL posicional (`--no-startup-window`); o modo Background usa `--headless=new`.
5. O Manager grava `cdp-port` e `cdp-pid` dentro do perfil. Consulta `http://127.0.0.1:<porto>/json/version`, valida o processo proprietário quando adota uma sessão, obtém `webSocketDebuggerUrl` e conecta ao WebSocket de controle do navegador.
6. Por CDP, habilita descoberta de targets, seleciona uma target de página sem ambiguidade, cria/seleciona a aba gerenciada e habilita `Page` e `Network`. O bridge observa o WebSocket iniciado pela própria página do jogo e extrai temporariamente os dados necessários ao handoff; não abre um segundo socket do jogo nessa fase.
7. `ConnectionManager` usa `tokio-tungstenite` para abrir a conexão Rust com o WebSocket do jogo, envia `hello` e espera um `welcome` real. Só então o Manager fecha a target do jogo e o Brave controlado e anexa o transporte Rust à conta. Se a transição falha, os estados de ownership mantêm ou restauram o Browser quando ele continua ativo.

Browser → Background não depende do local de `brave.exe`: os limites de ownership e o handoff vivem em `browser.rs`, `accounts.rs`, `connection.rs` e nos comandos de `lib.rs`. Nesta rodada não alteramos esses fluxos; fizemos revisão estática e rodamos os testes existentes de ownership/handoff junto com a suíte Rust.

## Resolver experimental

`BrowserRuntimeResolver` lê `POKEIDLE_MANAGER_BROWSER_RUNTIME_DIR` apenas como override local de POC. Quando definido, exige `brave.exe` e uma pasta de versão com `chrome.dll`, `icudtl.dat`, `Locales` e `resources`; caminho explícito inválido falha fechado e **não** recorre ao Brave instalado. Sem a variável, a descoberta anterior continua funcionando.

A precedência de produto pretendida continua futura: runtime gerenciado localizado pelos recursos do app → executável personalizado configurado pela pessoa → Brave externo como fallback de desenvolvimento. Só implementamos o ponto experimental necessário à POC; não criamos ainda configurações permanentes nem resolução por `resource_dir`.

## Instalação e árvore observadas

- Brave instalado detectado: `C:\Program Files\BraveSoftware\Brave-Browser\Application\brave.exe`.
- Versão exibida pela API CDP: Brave 1.96.61, Chromium 154.0.8037.98. A tag oficial de release confirma a correspondência.
- A pasta completa `Application` tinha 180 arquivos e 1.120.713.850 bytes (1.068,8 MiB / 1,044 GiB).
- A cópia de teste é `/.local-runtime-poc/Application` no checkout e está coberta por `.gitignore`. Foi feita cópia integral, sem retirar componentes. A estrutura inclui o executável raiz e diretório versionado com DLL principal, `icudtl.dat`, PAKs, V8 snapshot, DLLs de renderização/codec, `Locales`, `resources`, extensões, helpers e componentes auxiliares. Não assumimos que `brave.exe` sozinho baste.
- O conjunto tem elementos de serviço/helpers, `chrome_wer.dll` e `elevation_service.exe`; por isso a POC preservou a árvore integral. Não removemos arquivos nem tentamos reconstruir um runtime mínimo.

## Experimento reproduzível

Requisitos: Windows, runtime local copiado, Rust toolchain e Community build. A variável aponta para o diretório `Application` copiado:

```powershell
$env:POKEIDLE_MANAGER_BROWSER_RUNTIME_DIR = (Join-Path (Get-Location) '.local-runtime-poc\Application')
$env:POKEIDLE_MANAGER_POC_DATA_DIR = (Join-Path $env:APPDATA 'com.pokeidle.manager.browser-runtime-poc')
cargo test --manifest-path src-tauri/Cargo.toml --locked --no-default-features --features community-build copied_brave_runtime_supports_cdp_neutral_navigation_and_profile_restart -- --ignored --nocapture
```

O teste manual ignorado é deliberadamente fora da suíte padrão de CI. Ele chama o mesmo `WindowsBraveManager` usado pelo fluxo do Manager, abre um perfil novo dentro do AppData isolado pelo identifier do smoke, confirma target por CDP, envia `Page.enable`, `Network.enable` e `Runtime.enable`, navega apenas para `https://example.com/`, grava um valor inofensivo em `localStorage`, fecha pelo CDP e reinicia o Brave com o mesmo `user-data-dir`. Na segunda sessão confirma que o valor sobreviveu. Não usa o Brave global como fallback, jogo, login, conta, cookies ou dados normais do Manager.

Resultado observado: teste passou; endpoint `/json/version` e WebSocket CDP funcionaram; target de página encontrada; navegação terminou em `https://example.com/`; perfil/localStorage persistiram ao reiniciar. O profile de teste fica em `%APPDATA%\com.pokeidle.manager.browser-runtime-poc\profiles\neutral-<uuid>`, sob o AppData isolado definido em `tauri.runtime-poc.conf.json`. O profile é novo a cada execução. O teste cria apenas o diretório de perfil, não SQLite nem conta real, e não toca no AppData normal (`com.pokeidle.manager`).

Uma captura durante a página neutra mediu 7 processos Brave do runtime copiado, ~415,9 MiB de working set agregado e ~220,4 MiB de memória privada agregada. É uma amostra única, curta e sem login; não é orçamento de RAM para uso prolongado.

## Atualização, primeiro uso e tamanho

O teste abriu sem tela de welcome/primeiro uso e sem prompt de navegador padrão, usando os dois flags que já fazem parte do Manager. Nenhum serviço foi instalado/iniciado pela POC; os serviços Brave/Google updater existentes na máquina estavam parados durante a observação, e não houve processo updater dentro da árvore copiada. O Brave copiado iniciou e serviu CDP sem depender de um serviço updater ativo naquele teste curto. Isso não prova que nenhum pedido de atualização ocorreria em uma sessão longa: não fizemos captura de tráfego nem tentamos suprimir atualização com flags não validados.

O payload bruto observado é cerca de 1,044 GiB; portanto, se mantivermos a árvore completa, o instalador teria de carregar aproximadamente esse adicional antes da compressão, além do instalador atual. **Não construímos um MSI/EXE**, então o tamanho final compactado não foi medido. `resources` do Tauri 2 é o encaixe provável para uma árvore aninhada; `externalBin` é voltado a binários executáveis individuais/sidecars. A documentação do Tauri descreve `resources` como arquivos/diretórios recursivos e preserva sua árvore. A configuração do instalador oficial não foi alterada.

## Estado desta POC

- Nenhum binário do Brave foi adicionado ao Git; a cópia local está ignorada e fora dos arquivos propostos para PR.
- Nenhuma tag, release, build instalável ou pacote Brave foi criado/publicado.
- Nenhuma sessão de jogo foi iniciada; os testes automatizados de ownership continuam cobrindo Browser/Background, mas não fizemos login para testar handoff ao vivo.
- Esta comprovação vale para Windows x64 e para a versão instalada em 6 de outubro de 2026. Outras arquiteturas/canais ainda precisam de experimentos próprios.
