# Glossário e guia de estilo (pt-BR)

Atualizado a cada lote. Regra: o que está aqui vale para todos os arquivos; uma exceção precisa ser registrada na seção "Adaptações".
O texto original em inglês não é copiado para o repositório: as adaptações são citadas por `SPxx@offset` e descritas em português.

## Tom e voz
* Ficção policial cyberpunk dos anos 90, noir com humor. Português do Brasil natural: **você**, contrações e coloquial onde o original é coloquial
  ("tá", "pra", "né" com moderação). Nada de português europeu (nada de "tu", "ecrã", "telemóvel", "autocarro") e nada de formalidade dura.
* **Gillian Seed**: irônico, de língua solta, flertador; faz piada até na hora errada. Fala de si na primeira pessoa, sem cerimônia.
* **Metal Gear** (robô assistente): tagarela, prestativo, literal, com humor involuntário. Frases completas e um pouco formais, mas nunca duras.
* **Mika** (recepcionista/operadora): educada e delicada; trata Gillian por "Sr. Seed" até a intimidade crescer. Responde às cantadas com ironia leve.
* **Chefe**: seco, autoritário, chama todo mundo pelo sobrenome ("Seed"). **Harry**: velho teimoso e rabugento, coloquial ("garoto", "rapaz", "pô").
* Vozes: as falas dubladas continuam em inglês. A legenda fica **próxima do que se ouve** (mesma ideia, mesma ordem, mesmas palavras-chave), para quem
  entende um pouco de inglês não estranhar. As linhas com voz aparecem no despejo de contexto como `vNNNN`.
* Erros de digitação do original ("eachother", "your not serious") não se reproduzem: traduz-se o sentido.
* Aspas: usar `"` e `'` (não há aspas curvas na fonte). Reticências: `...` como no original. Travessão: usar `-` ou reformular.
* Datas dos arquivos de pessoal: dia/mês/ano (25/05/1959).

## Nomes próprios (ficam como estão)
Gillian Seed, Jamie Seed, Jean-Jack Gibson (Gibson), Katrina, Mika, Harry (Benson), Metal Gear, Napoleon, Isabella (Velvet), Freddy (Nielsen),
Ivan (Rodriguez), Random Hajile (Random), Chin Shu Oh, Jordan (o computador), Neo Kobe, Alton Plaza, Outer Heaven, Liquid Sky, Alpha-One,
Seed Garden, Plato's Cavern (Caverna de Platão só em frase corrida), todos os nomes de funcionários da Konami (SP05) e os cameos (Dracula, Simon Belmont,
Sparkster, Goemon, Contra, Freeman).
Lugares com nome + substantivo comum: o substantivo vai para o português e o nome fica: **Hospital Queens**, **Hospital Oleen**, **Restaurante Buffalo Cuisine**.

## Termos do jogo (decisão única)
| Original | pt-BR | Observação |
|---|---|---|
| SNATCHER | SNATCHER | nome da ameaça; plural "os SNATCHERS"; "bioroid SNATCHER" -> "SNATCHER bioroide" |
| bioroid | bioroide | |
| JUNKER / Junker | JUNKER / Junker | nome da organização e de seus membros; plural "os Junkers"; sigla J.U.N.K.E.R. fica em inglês (nome oficial gravado na placa) |
| Junker Headquarters | Quartel-General Junker (QG) | "sede" também serve em frase corrida |
| Chief | Chefe | nome do falante: "Chefe" |
| runner | runner | termo de função dos investigadores Junker, destacado em cor como no original |
| engineer | engenheiro | |
| bounty hunter | caça-recompensas | plural "os caça-recompensas"; no menu: CAÇA-RECOMPENSAS |
| blaster | blaster | arma; "a blaster" |
| Justifier | Justifier | pistola de luz; "o botão START da pistola" |
| Metal Gear / navigator | Metal Gear / navigator | "navigator" fica como empréstimo (modelo de robô assistente), minúsculo, "o navigator" |
| Turbocycle | Turbocycle | o veículo, feminino: "a Turbocycle" |
| videophone | videofone | |
| Junker I.D. | carteira Junker | no menu: CARTEIRA JUNKER |
| investigation (save/quit) | investigação | "continuar investigação" |
| montage (retrato do suspeito) | retrato falado | no menu: RETRATO FALADO |
| Mr. / Mrs. / Miss | Sr. / Sra. / Srta. | |
| Seed (tratamento) | Seed / Sr. Seed | |
| Nomes dos falantes sem nome próprio | Chefe, Motorista, Funcionário, Homem, Mulher, Passante, Porteiro, Gerente, Cliente, Papagaio, Alto-falante | ver `names.json` |

## Verbos e rótulos do menu de comandos (iguais em todos os arquivos; largura máxima 224 px)
OLHAR, INVESTIGAR, FALAR, PERGUNTAR, MOSTRAR, USAR, IR (MOVE), ENTRAR, SAIR, SALVAR, NÃO SALVAR, PERTENCES, SOBRE X ("ASK > ABOUT X"), PERGUNTAR DE NOVO,
OUTROS ASSUNTOS, CAÇA-RECOMPENSAS, CONTINUAR INVESTIGAÇÃO, SAIR DA INVESTIGAÇÃO, USAR METAL GEAR, VIDEOFONE, ÁREA, AO REDOR.
O dicionário de rótulos comuns está em `menu.json` (chave = hash do rótulo original); `snt i18n-apply-menu` o aplica a todos os arquivos.
Rótulos em MAIÚSCULAS, sem artigo quando possível ("SOBRE MIKA", "PORTA", "FOTO DO GIBSON").
Menus de digitação (teclado/teclado numérico, 9 ou mais itens, SP05 e SP09, ...) **não se traduzem**: o jogador digita letras sem acento e o jogo compara com o texto.

## Regras de escrita
* Quebras de linha `{nl}` mantêm o ritmo do original; usar o limite em pixels do validador (fala 208 px, narração 176 px, menu até x=240).
* Cores `{cN}` e botões `{btn:XX}` ficam nos mesmos pontos lógicos (mesma sequência de cores). `{auto}` e `{page}` nunca mudam de lugar na lógica da cena.
* Mais páginas que o original: evitar; é obrigatório não aumentar em linhas com `{auto}`.
* Gênero: Gillian é homem (adjetivos no masculino quando fala dele); Mika, Katrina, Jamie, Isabella são mulheres; Metal Gear é "ele" (robô).
* Palavrões e insinuações: manter a intenção e o nível; adaptar o trocadilho quando o inglês não funciona.

## Adaptações não literais (uma linha por motivo)
* SP06@0df9, SP06@0e3b: a sigla JUNKER e sua expansão em inglês ficam como estão (nome oficial; as iniciais coloridas formam a palavra).

## Regras acrescentadas pela revisão de SP06 (nota 4/5 nas duas partes)
* Harry nunca usa "receio", "senhor" nem imperativos formais: "Infelizmente", "Dá pra", "Para com isso", "Tá". Gillian também evita "receio".
* Mika diz "o senhor"/"Sr. Seed" na recepção e passa a "você" a partir do tour pelo QG.
* A identidade Junker é sempre "carteira Junker" (menu e diálogo); "cartão" só onde o inglês diz "card".
* Itens de salvamento: SAVE n = ARQUIVO n (combina com "número de arquivo"). TURBOCYCLE BODY = CARROCERIA.
* Tiro: "wasted shots" = "tiros perdidos"; "accuracy" = "pontaria" (evitar "precisão precisa"). "Come on, let's go" = "Anda, vamos".
* "ceramoplastics" = "plásticos cerâmicos". "Caça-recompensas" no menu leva artigo como os irmãos: SOBRE OS CAÇA-RECOMPENSAS.
* "at face value" (piada com o menu ROSTO DA MIKA) = "pelo valor de face".
* Siglas JORDAN e JUNKER: a expansão em inglês fica (nomes oficiais), SP06@0df9/@0e3b/@502d.

## Regras acrescentadas pela revisão de SP07 (4/5)
* "head twisted off" = "torceram a cabeça dele até arrancá-la". "screen" (de SNOW-9) = "cortina de fumaça". Temporizador da bomba = "cronômetro" (termo fixo).
* Metal Gear pode usar "Receio que..."; a proibição vale só para Harry e Gillian. Lembretes de salvar: "Convém SALVAR" / "você deve SALVAR".
* O rótulo SAIR é compartilhado entre o verbo (IR > SAIR) e o objeto "saída" (INVESTIGAR): mantido SAIR.

## Regras acrescentadas pela revisão de SP00, SP02 e SP03 (4/5 e 4/5)
* Peça de xadrez "queen" = "rainha" (a pista do Hospital Queens depende disso). Mensagens de salvamento: só "salvar", nunca "gravar".
* Gillian usa imperativos coloquiais ("Peraí", "Espera aí"), nunca "Espere". Metal Gear usa "o/lhe" com verbos formais e evita "te"; depois de "se" usa o subjuntivo.
* Konami (SP02): títulos de jogos e plataformas ficam em inglês (Genesis, SNES); "R&D" = "P&D"; "Loveline" = "Linha do Amor"; "prank call" = "passar trote".
* Adaptações de SP02 (piadas): medidas convertidas ao sistema métrico (1,78 m, 97-61-91); "shwinggg" = "bóiiing"; "Pussy SNATCHERS" = "SNATCHERS gatinhas" (gatinha = bichano e mulher bonita);
  "bait your own hook" = "se virar com as próprias mãos"; "toenail surgery" = "cirurgia de unha encravada"; "Out, vile jelly" (Rei Lear) = "vil geleia";
  Gilligan's Island ("aquela ilha", "Mary Ann") mantido; a fala meta sobre a tradução diz "traduzido para o inglês e legendado em português".
* Letras de música (Buddy Guy), o cumprimento em japonês e as reticências de silêncio de SP02 ficam sem tradução (8 textos).
* "Face to face" (memória de Harry, SP03@38b4 e @390f) fica em inglês: o jogador pode ter de digitar a frase.

## Regras acrescentadas pela revisão de SP04 (4/5)
* "alamid" = "aramida". "The Catastrophe" no meio da frase = "a Catástrofe" (artigo minúsculo; maiúscula só entre aspas/títulos). Anos escritos '47 ficam "'47".
* Rótulos "QUIT X" = "SAIR DO/DA X" com o objeto (SAIR DO JORDAN, SAIR DO RETRATO, SAIR DO ARQUIVO). "POSSIBLE MATCH" = "POSSÍVEL CORRESPONDÊNCIA". "Half-Year War" = "Guerra dos Seis Meses".
* "bylaws" = "estatutos" (menu e título no plural). "Immigrant problem" = "problema imigratório". "lymph nodes" = "linfonodos". "cut-rate prices" = "preços baixíssimos".

## Regras acrescentadas pela revisão de SP05 (4/5)
* Fichas de pessoal (SP05): rótulos fixos: Aniversário, Signo, Tipo sanguíneo, Personalidade, Apelido, Jogo favorito, Mensagem, Número do videofone, Idade, Alt./Altura, Peso, Estado civil, Histórico, Família, Endereço; "KIA" = "morto em ação"; "Class N ID" = "identidade de Classe N".
  Personalidade e apelido concordam com o gênero da pessoa (Taeko, Kathleen, Kathy, Kimberley, Marlyse, Eiko, Xyla = feminino). "anti-SNATCHER" com hífen.
* Nos arquivos de SP05 os nomes aparecem como no texto original: "Jean Jack" (sem hífen) e "Chin Sho Oh". Erros de digitação do inglês ("Dormitore") não se reproduzem.
* Citações da cultura pop usam a versão brasileira conhecida (Sailor Moon: "Em nome da Lua, eu vou castigar você!"). "contrary" = "do contra".

## Regras acrescentadas pela revisão de SP08 (4/5 e 4/5)
* Harry e Gillian usam imperativos coloquiais: "Para com isso", "Esquece", "Relaxa", "Me deixa em paz", "Escuta", "Olha", "Dá uma olhada" (nunca "Pare/Esqueça/Relaxe/Escute/Olhe/Dê").
* "console" (DO SOMETHING > CONSOLE) = CONSOLAR. "make advances" = "dar em cima de". "you know" final = "sabe". Peças de xadrez ("rei", "rainha") em minúsculas. Salas ("Sala dos Detetives", "Sala do Chefe") em maiúscula inicial.
* Efeitos sonoros: "<<sniff>>" cheirando = "<<fungada>>"; chorando = "<<choro>>"; soluço = "<<hic>>".
* SP08@42b7 (expansão da sigla JUNKER na placa) fica em inglês. "Insectors" fica como nome do robô.
* Quando o jogador digita a resposta num teclado em inglês (SP09, enigma da marca de nascença de Katrina), os avisos dizem "(em inglês)" e a ficha de Katrina em SP05 traz as palavras
  a digitar entre parênteses (BIRTHMARK, HEART-SHAPED, INNER THIGH).

## Regras acrescentadas pela revisão de SP09 (4/5)
* Pistas vagas no inglês ficam vagas: a peça "ao lado do rei" não é nomeada ("a que fica ao lado do rei"); "rainha" só onde o inglês diz "queen" (SP09 "falta a rainha preta").
* Gillian e Katrina usam imperativos coloquiais mesmo em gritos e cantadas ("Abre aí!", "Cala a boca!", "Anima aí", "Toma, deixa eu..."), e pronomes depois do verbo ("conhece ele", "receber você"). "o/lhe" só para Metal Gear.
* Alice é "cachorra" (feminino em todas as falas); a garrafa de protetor solar é "garrafa" (não "frasco"); "Jean-Jack Gibson" com hífen nos diálogos (as fichas de SP05 mantêm "Jean Jack"). "<<sob>>" = "<<choro>>".
* SP09: nos enigmas digitados o aviso é "(em inglês)"; os rótulos de teclado (números, WRONG, MOLE, ...) ficam em inglês.

## Regras acrescentadas pelas revisões de SP10 e SP11 (4/5 e 4/5)
* Todos os falantes informais (Gillian, Harry, Katrina, Napoleon, porteiros, funcionários, transeuntes, clientes) usam imperativos coloquiais ("Para", "Cala a boca", "Toma", "Vai", "Sai", "Anda", "Peraí", "Dá uma olhada", "Desculpa").
  "Pare/Espere/Tome/Saia" formais só para Metal Gear, Chefe e Mika. Metal Gear não usa "tá?": fecha com "está bem?". "Nah" = "Não" / "Que nada", nunca "Nada".
* "you know" final = "sabe" (nunca "sabia"). PICK UP WOMAN = PAQUERAR. GET FORTUNE TOLD = CONSULTAR A VIDENTE. LEAVE ON (máscara) = MANTER. "transvestite" (homem de maquiagem) = "homem vestido de mulher".
  "Loveline" (impresso na propaganda) = "Linha do Amor".
* A varredura mecânica: `work/i18n/sweep.py` (não versionado) trocou imperativos formais por coloquiais nas falas de personagens informais; `snt i18n-rewrap` refaz quebras que ficaram largas.

## Regras acrescentadas pela revisão de SP12 (4/5)
* Porteiro: trata por "você" (usa "senhor" só onde o inglês diz "sir"); porteiro hostil ou irritado usa imperativos coloquiais ("Some", "Desiste", "Põe").
* "looks great on you" (objeto) = "caiu muito bem em você". "special effects shop" = "estúdio de efeitos especiais". "every hour" = "a cada hora" ("toda hora" significa sempre). "report on" = "fazer relatórios sobre".
* Metal Gear nunca diz "tá?" nem "tá bom": fecha com "está bem?" (varredura feita em SP00-SP12).

## Regras acrescentadas pela revisão de SP13 (4/5)
* "Nope"/"Nah" = "Não" ou "Que nada", nunca "Nada". "Watch it!" (aviso) = "Cuidado aí!". "pulse rate" = "batimentos". "simple" (ingênuo, em provocação) = "simplório".
* Mulheres informais e paqueradoras (Isabella, Katrina) usam imperativos coloquiais ("não fica", "não deixa", "me poupa") e pronome depois do verbo ("vi ele").
* Menu "SOBRE X" nunca leva artigo (SOBRE ISABELLA, SOBRE MIKA), exceto o rótulo fixo SOBRE OS CAÇA-RECOMPENSAS.
* Quando um personagem cita uma fala anterior (SP13@407c cita @1f13 e @22ca), as falas originais devem ter exatamente a mesma redação.
* O gerente do clube trata Gillian por "o senhor" em todas as falas.
