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
* Vozes: as falas dubladas continuam em inglês e **o jogo original nunca mostra texto junto com uma fala dublada** (a caixa de mensagem fica escura e vazia enquanto o áudio toca; todos os
  1146 trechos são seguidos de espera). Portanto nenhuma linha de `SPxx.json` legenda uma fala: as marcas `vNNNN` do despejo de contexto só dizem que um `SAY` vem logo antes ou depois de um trecho
  (não são a transcrição dele). As legendas das falas ficam em `voice.json` (uma por trecho, `docs/voice-subtitles.md`) e seguem as regras da seção "Legendas das falas" abaixo.
* Erros de digitação do original ("eachother", "your not serious") não se reproduzem: traduz-se o sentido.
* Aspas: usar `"` e `'` (não há aspas curvas na fonte). Reticências: `...` como no original. Travessão: usar `-` ou reformular.
* Datas dos arquivos de pessoal: dia/mês/ano (25/05/1959).

## Legendas das falas (`voice.json`)
* A legenda acompanha o que se **ouve**: mesma ideia, mesma ordem, palavras-chave do inglês reconhecíveis (quem entende um pouco de inglês não deve estranhar). Pode condensar quando o ritmo exigir,
  nunca inventar nem acrescentar informação. Tom e vocabulário são os mesmos das linhas de `SPxx.json` (Gillian irônico, Metal Gear tagarela e formal, Harry rabugento, Mika educada...), com os mesmos termos do glossário.
* Velocidade de leitura: no máximo 17 caracteres por segundo, contando o tempo do trecho mais a permanência extra (até 1,5 s) que o motor concede; `snt voice-check` reprova o que passa disso.
  Trechos rápidos pedem frases enxutas (cortar "bem," "então," repetições, preferir a forma curta).
* Falas curtas e exclamações com palavra ("Cuidado!", "Evacuar!") ganham legenda; gemidos, gritos, espirros, risadas e efeitos (sem palavras) não ganham (`voice.json` não os lista).
* Nome do falante: só quando o contexto deixa claro (a linha de nome aparece como numa mensagem); na dúvida, sem nome. Número do falante = o de `snt i18n-names`.
* Páginas: 3 linhas por página (4 quando isso evita quebrar no meio de uma expressão) (o motor divide o tempo do trecho entre as páginas pelo tamanho do texto): fim de página em fim de frase sempre que possível. Quebras de linha com `{nl}` (ou `~` no importador).
* Colchetes e efeitos sonoros do roteiro não se repetem: a legenda é só a fala.

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

## Regras acrescentadas pela revisão de SP14 (4/5)
* "neighborhood" = "bairro" (nunca "vizinhança"). "Alpha-One" é feminino ("a Alpha-One", é uma rede): "da/na/à Alpha-One" em todos os arquivos. "agency" sozinho = "agência" (minúsculo); "Agência Junker" com maiúscula.
* "farto de" é livresco: usar "de saco cheio de", "enjoar de", "cansado de" para falantes informais. Freemen e moradores de rua: "você" com imperativos coloquiais ("Junta com a gente"), "o que você pensa que tá fazendo".
* Rótulos "SOBRE X" com nome de pessoa não levam artigo (SOBRE IVAN, SOBRE FREDDY, SOBRE ISABELLA, SOBRE GIBSON).
* Sujeito feminino concorda ("a banheira ... suja, desbotada"). "freemen" fica como termo estrangeiro (nome do falante: Freeman).

## Regras acrescentadas pelas revisões de SP15, SP16 e SP17 (4/5, 4/5, 4/5)
* "Turbocycle" sempre com maiúscula e feminino ("a Turbocycle", "as Turbocycles"). "system bath" = "banheira/unidade de banho modular". "GO OUT FRONT" = "SAIR DO PRÉDIO". INVESTIGATE X é sempre INVESTIGAR X (nunca EXAMINAR). "desde cedo" (nunca "desde de manhã").
* Civis (Lisa) tratam Gillian e Metal Gear por "o senhor/os senhores"; Gillian responde "a senhora" durante toda a cena de SP15. "Dormitory Okamoto" = "Dormitório Okamoto" na fala dublada (SP05 mantém o texto da ficha).
* Mensagens de salvamento: "salvamento anterior", "dados salvos"; "sobrescrevê-lo" com o pronome. "sistemas de prevenção ao crime" (não "antiassalto"). "Que tal...?" termina com interrogação.

## Regras acrescentadas pelas revisões de SP18, SP19 e SP20 (4/5, 4/5, 4/5)
* Metal Gear só usa imperativos formais ("Pare de", "Faça", "Controle-se", "Olhe"), nunca "Para de", "Faz", "Olha" (varredura feita em SP00-SP20). Gillian trata Metal Gear por "você", nunca "o senhor".
* Napoleon (rude, informal): "tem" em vez de "há", "a gente" em vez de -mos, "pra". Jamie (informal): pronome depois do verbo ("atacar ele"), sem "o/lhe".
* "tissues" = "lenços de papel". "mumbling" = "murmurar/resmungar". "trigger happy" = "dedo nervoso". "human-type SNATCHERS" = "SNATCHERS de tipo humano". "put the machine back on intermediate" = "colocar a máquina de volta no nível intermediário". "rapid fire" = "tiro rápido". "wasted shots" = "tiros desperdiçados". "no cigar" = "não foi dessa vez".
* Senhas digitadas do Napoleon ("The hundred days reign", "The revolution is ended") ficam em inglês entre aspas; as respostas do menu ficam em inglês; as reações dele traduzem o sentido (ortografia, tempo verbal).

## Regras acrescentadas pela revisão de SP21 (4/5 e 4/5)
* "snatched" (pessoa substituída por SNATCHER) = "raptado/a" (nunca "capturado"). "Fool" (Chefe) = "Tolo", "Idiot" = "Idiota". Menu "OWN PAST" = "MEU PASSADO".
* Harry, Gillian e Napoleon não usam "Receio que"/"Infelizmente" formal: "Acho que", "Não posso". Gritos de Gillian: "Responde", "Anda, sai daí".
* Metal Gear: "Não há como ..." (nunca "Não tem como"). Salas: "Sala do Chefe", "Sala dos Detetives", "sala do computador" (igual aos rótulos do menu).
* "Outer Heaven" é masculino: "do Outer Heaven", "o Outer Heaven" (após "para/a" fica sem artigo). Mensagem de arquivo: "salvamento anterior"; "Posso sobrescrevê-lo com os novos dados?" em todos os arquivos.
* "SOB A MESA PRÓXIMA / SOB A MESA AO FUNDO" (rótulos de esconderijo); "patient record" = "ficha do paciente" sempre.

## Regras acrescentadas pelas revisões de SP22, SP23, SP24 e SP25 (4/5 cada)
* "Alton Plaza" é feminino ("a Alton Plaza", "na/à/da Alton Plaza"). "Não admira que" é lusitano: usar "Não é à toa que". GET FORTUNE TOLD = CONSULTAR A VIDENTE em todos os arquivos. NEARBY PEOPLE = PESSOAS PRÓXIMAS. "insurance" = "seguro".
* O quadro "Face to face" da sala do Chefe é "quadro"; a foto escondida da Praça Vermelha é "foto". Mensagens de memorando: "o segredo do quadro no escritório do Chefe". Nas descrições de itens vale "escritório do Chefe"; o rótulo de menu da sala é "Sala do Chefe".
* "CALL" (gritar o nome de alguém na casa do Gibson, SP25) = CHAMAR; "CALL" ao telefone = LIGAR. Listas de números de telefone mantêm um item por linha, como no inglês (usar quebras forçadas).
* Gillian sempre põe o pronome depois do verbo ("procurar ela", "encontrar ela", "vejo ele") e chama a Katrina com imperativo coloquial ("Responde"). "Time is of the essence" = "Não temos tempo a perder". "Yeah. Open it!" (dublada) = "Sim. Abre!".

## Regras acrescentadas pelas revisões de SP26 e SP27 (4/5 e 4/5)
* Gillian e Random falam "tem/não tem" (não "há") na fala solta e nas confirmações ("..., tem?"); Metal Gear mantém "há". "De que tipo de X você está falando?" com a preposição. "reception desk/booth" = "balcão (da recepção)". "sala do fundo" para a sala no final do corredor.
* Napoleon (e o porteiro quando é ele): "Relaxa", "a gente vai", "Pensa", "Tá bom"; nunca "Não se preocupe" nem a forma em -mos. Falantes informais dizem "com certeza" (não "definitivamente"). "untrustworthy" = "que não merece confiança".
* "Outer Heaven" masculino também entre aspas ("fósforos do "Outer Heaven""). "Face to face" (quadro e bilhete) = "quadro"; "foto" só para a foto da Praça Vermelha e a do templo. Varredura global feita em SP00-SP28 depois de cada lote (as cópias de strings iguais podem trazer a versão antiga).

## Regras acrescentadas pelas revisões de SP28 e SP29 (4/5 e 4/5)
* Katrina (18 anos): "a gente", "pra", "meu pai" (sem artigo antes do possessivo), "com certeza"; nunca "estejamos/podemos". Erros de pontuação do original (".." no fim) não se reproduzem.
* Quando o diálogo cita um comando de menu, usa o verbo exato do rótulo (PRAY TO GOD = REZAR: "Rezar!"). Temperaturas em Fahrenheit passam a Celsius (68-72 F = 20-22 °C). Respostas digitadas continuam em inglês e os avisos dizem "(em inglês)".

## Regras acrescentadas pela revisão de SP30 (4/5)
* "reception booth/desk" = sempre "balcão de recepção" (nunca "cabine") no diálogo e no menu. Gillian e Random: "Não tem dúvida", "Não tem sinal de...", "com certeza"; "tem cheiro de" (não "cheira a"); imperativos coloquiais ("Fica calmo", "Não baixa a guarda"); Gillian: "identificar eles", "se a gente tivesse".
* Siglas nas listas técnicas do Metal Gear mantêm a expansão do inglês traduzida (RMN com "ressonância magnética nuclear", STM com "microscópios de tunelamento").

## Regras acrescentadas pela revisão de SP32 (4/5)
* Metal Gear mantém "há/Haverá", nunca "tem/vai ter", mesmo em piadas. Gillian com Metal Gear: imperativos coloquiais ("não baixa a guarda", "liga a sua luz") e "pra gente" em vez de -mos na fala direta. "nosy" = "enxerido". Autocorreção "er," = "digo,".
* Taxista: "o senhor" em toda a cena (só a revelação hostil pode trocar). "Junk Team" = "Equipe de Limpeza Junker"; "Kyoto Summit" = "Cúpula de Kyoto". "Que tal ...?" sempre com interrogação.

## Regras acrescentadas pelas revisões de SP33, SP34 e SP35 (3->4/5 depois das correções, 4/5, 4/5)
* Balcão da recepção do QG: "balcão de recepção"; "booth" sozinho = "balcão"; "shutter" = "cortina de aço" (rótulos CORTINA DE AÇO, ABRIR CORTINA); "abrir a cortina (do balcão)"; "atrás do balcão". "cabine" só para as cabines do estande de tiro. "UNDER FRONT DESK" = "SOB A MESA PRÓXIMA".
* Salas citadas no diálogo coincidem com o menu: "sala do computador", "Sala dos Detetives", "Sala do Chefe". "Gillian's apartment" = "seu apartamento" nas falas de viagem; "10-4!" = "Positivo!"; "This is it!" = "É isso!".
* Jamie (casual): "tem" no lugar de "há", "a gente" com verbo no singular ("a gente deve", "a gente pode"), nunca "devemos/ficarmos/tenhamos" nem "o senhor" para o Gillian. Napoleon: "a gente pode/não pode". "genetic manipulants" = "animais geneticamente modificados". Varredura de "Receio que" nas falas de Gillian, Napoleon, Harry, Random, Jamie e Katrina feita (SP00-SP35).

## Regras acrescentadas pelas revisões de SP36, SP37 e SP38 (4/5 cada)
* Respostas de um mesmo menu de quiz são da mesma classe gramatical (ESPIRRO, TOSSE, BOCEJO, SOLUÇO). "container" = "contêiner/contêineres" em menus e diálogo. "back room" = "sala do fundo".
* Mika usa imperativos formais ("Pare", "Não fique", "Aguente firme") em todo o jogo; Gillian e Jamie usam "Não se preocupa", "Aguenta firme", "tem como/não tem como", "a gente achou". "Não se preocupe" é de Metal Gear, do Chefe e da Mika.
* Patentes: General, Coronel/coronel, Major/major, Capitão/capitão, Segundo-Tenente/segundo-tenente (com hífen). "has a nice ring to it" = "soa bem". Imagens de beisebol ("bottom of the ninth") viram "minutos finais da partida".
