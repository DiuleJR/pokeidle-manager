# Como contribuir

Obrigado por ajudar a melhorar o Pokeidle Manager. O código-fonte deste repositório é a edição comunitária oficial do projeto. Sugestões e relatos podem ser enviados pelas opções de Issues do GitHub.

## Antes de propor uma alteração

- Mantenha as alterações focadas e explique o impacto para quem usa o aplicativo, incluindo efeitos sobre compatibilidade ou migração de dados.
- Não envie bancos de dados de contas, perfis do navegador, logs, cookies, tokens, capturas de tela com dados pessoais, arquivos de ambiente local nem material privado de recuperação.
- Não adicione artes, sprites, logotipos ou outros ativos de terceiros sem autorização documentada para redistribuição. Consulte [Ativos e ícones](docs/ASSETS.md).
- Preserve o funcionamento local do aplicativo e informe claramente qualquer nova conexão externa ou coleta de dados.
- Nunca exponha o servidor mobile de desenvolvimento ou o servidor auxiliar do Vite a uma rede não confiável.

## Verificações locais

Use versões compatíveis do Node.js/npm e da cadeia de ferramentas Rust e execute:

```powershell
npm ci
npm run check
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
```

### Smoke test seguro do aplicativo desktop

No Windows, inicie o aplicativo comunitário sem reutilizar o perfil normal:

```powershell
npm run tauri:dev:smoke
```

Esse comando mescla `src-tauri/tauri.smoke.conf.json` à configuração Tauri padrão e usa o recurso `community-build`. O `identifier` isolado (`com.pokeidle.manager.smoke`) faz o Tauri usar `%APPDATA%\com.pokeidle.manager.smoke`, separado do diretório normal `%APPDATA%\com.pokeidle.manager`. Antes de executar, confirme que o diretório de smoke ainda não existe; o primeiro uso cria nele o banco e pode criar cache/perfil temporário do WebView ou navegador. Não copie bancos SQLite, perfis, cookies, configurações, contas, cache ou logs de outra instalação. Feche a janela normalmente ao terminar. Só remova o diretório de smoke depois de confirmar que o processo está encerrado e que o caminho corresponde exatamente ao identificador de smoke.

Para gerar uma versão comunitária empacotada para Windows, use o comando explícito de compilação comunitária do repositório e confira o artefato localmente.

## Envio de contribuições

Use o fluxo de contribuição documentado neste repositório. Inclua testes ou verificações manuais, informe limitações e não inclua arquivos de compilação gerados nem dados pessoais nos commits. Ao enviar uma contribuição, você a disponibiliza sob a licença do projeto, salvo se os mantenedores documentarem outra condição.
