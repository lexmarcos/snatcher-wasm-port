# Snatcher Web Port

[Read in English](README.md)

Jogue **Snatcher** (a versão de 1994 para Sega CD, americana) no seu navegador, usando a sua própria cópia do jogo.

**Jogue no navegador: https://markzuel.com/snatcher-wasm-port/**

Isto não é um emulador. O motor do jogo foi refeito do zero para rodar direto no navegador. Ele lê o disco original
e roda o jogo como o console rodava, com as mesmas imagens, músicas e vozes.

**Nenhum arquivo do jogo vem junto.** Você precisa da sua própria imagem de disco de Snatcher (USA, Sega CD).

## O que você tem

* O jogo inteiro, da tela de título ao final
* Inglês (o texto original) ou português do Brasil, com troca a qualquer momento
* Legendas em português nas cenas com voz e nas falas das músicas do CD
* Teclado, controle (Xbox, PlayStation, Switch) e mouse nas cenas de tiro
* Celulares e tablets: controle na tela, ou toque na própria imagem
* Salvamento automático, para continuar de onde parou (dá para desligar)
* Seus arquivos nunca saem do seu computador: tudo acontece dentro do navegador

## Como jogar

1. Abra https://markzuel.com/snatcher-wasm-port/ (ou rode a página você mesmo, veja "Para desenvolvedores" abaixo).
2. Aperte **Importar imagem do disco** e escolha o arquivo `.cue` **junto com** o(s) arquivo(s) `.bin`. Você também pode arrastá-los para a página.
3. Espere alguns segundos enquanto o disco é conferido e copiado para o navegador.
4. Aperte **Jogar**.

Nas próximas visitas o jogo já está lá: é só apertar **Jogar** ou **Continuar de onde parou**.

A cópia guardada no navegador ocupa cerca de 1,1 GB. Você pode removê-la quando quiser na página inicial
(**Remover os arquivos e limpar o site**).

**Tem um arquivo `.chd`?** Ele ainda não é aceito. Converta antes com
`chdman extractcd -i "Snatcher (USA).chd" -o snatcher.cue -ob snatcher.bin`.

## Controles

| Teclado | Ação |
|---|---|
| Setas | Mover / escolher |
| Z, X, C (ou A, S, D) | Botões A, B, C |
| Enter | Start |
| Esc | Menu e pausa (som, tela, controles, saves) |
| F | Tela cheia |
| M | Sem som |

Nas cenas de tiro, mire com o mouse e clique para atirar. O controle funciona assim que você aperta um botão,
e os botões podem ser trocados no menu.

## No celular ou no tablet

O jogo funciona em celulares e tablets (Android, iPhone, iPad). Dá para jogar de dois jeitos, e você escolhe um no menu (botão **Menu**, depois a aba **Toque**):

* **Controle na tela** (o padrão no celular): um controle de Sega CD é desenhado na tela, com o direcional, os botões A, B e C, Start e Menu. Dá para segurar dois botões ao mesmo tempo. Nas cenas de tiro o direcional mira nos nove alvos, A saca a arma e C atira.
* **Tocar no jogo**: nenhum controle é desenhado. Você toca na própria imagem. Toque num comando ou numa tecla para escolher, toque numa mensagem para continuar (segure para ler mais rápido) e toque num alvo nas cenas de tiro para atirar ali. Os botões **Voltar** e **Start** só aparecem quando são necessários, e um toque com dois dedos também é Voltar. Na sala escura, arraste o dedo para mover a luz.

Dicas:

* A imagem fica maior com o celular na horizontal, mas os dois jeitos funcionam.
* No iPhone, o Safari não mostra a página em tela cheia. Toque em Compartilhar, depois em **Adicionar à Tela de Início**, e abra o Snatcher pelo ícone: ele roda sem as barras do navegador. No Android e no iPad o botão **Tela cheia** da aba Toque funciona.
* Para importar o seu disco, toque em **Importar imagem do disco** e escolha o arquivo `.cue` junto com o(s) arquivo(s) `.bin`. No iPhone, toque em **Selecionar** antes, para poder escolher mais de um arquivo.
* A cópia guardada no navegador ocupa cerca de 1,1 GB. Se o seu celular tiver pouco espaço livre, a página avisa antes de começar (dá para desmarcar a música do CD e economizar 490 MB).
* A tela fica ligada enquanto você joga. Na aba Toque dá para desligar isso e ligar a vibração.

## Para desenvolvedores

O site público é construído a partir deste repositório por um fluxo do GitHub Actions a cada push na `main`.
Para rodar a página no seu próprio computador você precisa do [Rust](https://rustup.rs) e do [Bun](https://bun.sh). Depois, num terminal, dentro desta pasta:

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.129
scripts/build-web.sh
bun web/server.ts
```

Abra http://localhost:5300 no navegador (testado no Chrome). Para gerar a mesma pasta estática que o site usa
(`build/site/`), rode `scripts/build-static.sh`.

As notas técnicas, a engenharia reversa do jogo original e as ferramentas de teste estão em [`docs/`](docs/) (em inglês).
Bons pontos de partida: [`docs/PROGRESS.md`](docs/PROGRESS.md) e [`docs/re/web.md`](docs/re/web.md).

## Aviso legal

Este é um projeto independente de fã. Não tem ligação com a Konami nem com a Sega, nem o apoio delas.
Snatcher é © Konami. Sega e Sega CD são marcas da Sega. Para jogar você precisa da sua própria cópia do jogo.

O código e os textos do próprio projeto estão sob a licença MIT ([`LICENSE`](LICENSE)), que não dá nenhum direito sobre o jogo.
A fonte DotGothic16 em `web/fonts/` está sob a SIL Open Font License ([`web/fonts/OFL.txt`](web/fonts/OFL.txt)).
