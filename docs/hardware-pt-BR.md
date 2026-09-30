# Referências de instalação de dobradiças

## Área de Ferragens

O painel de Ferragens começa com **Adicionar ferragem ▾** e depois tem uma
seção por tipo. As seções aparecem sempre, mesmo vazias, e cada uma tem seu
**+**:

| Seção | Lista | **+** adiciona |
|---|---|---|
| **Portas e dobradiças** | Cada porta com suas dobradiças, depois as dobradiças fora de uma porta | Uma porta (na peça selecionada) ou uma dobradiça avulsa |
| **Gavetas e corrediças** | Uma linha por gaveta com o código da corrediça | Corrediças na gaveta selecionada |
| **Pés** | Pés de catálogo | Um pé embaixo do móvel selecionado |
| **Outras ferragens** | Caixas dimensionadas para puxadores, trilhos e afins | Uma caixa de 100 mm que você pode redimensionar e mover |
| **Modelos do catálogo** | Todos os modelos fixados no projeto: dobradiças, corrediças e pés, com quantos itens usam cada um | Abre o catálogo |

Adicionar cria o item na hora, em um passo de desfazer, e o abre no inspetor
para você ajustar:

- **Pé.** Usa o último modelo de pé usado, fica embaixo do móvel selecionado a
  20 mm do canto e desce a própria altura abaixo do chão. O inspetor oferece
  **Levantar <móvel> N mm**.
- **Corrediças.** Vão na gaveta da seleção, com a última família usada (TT45
  por padrão). Se a gaveta já tem corrediças, elas são abertas.
- **Porta.** A peça selecionada é pendurada na lateral mais próxima, com as
  dobradiças soltas dela ou um conjunto padrão.
- **Dobradiça.** Entra na porta da peça selecionada.

Uma janela de escolha só abre quando algo precisa ser escolhido antes: por
exemplo, corrediças sem nada selecionado, ou um pé quando não há modelo de pé
disponível.

Todo item abre num inspetor editável, aqui e em **Projeto 3D**. Clique na linha
dele, no link embaixo de uma peça ou grupo em Projeto 3D, ou (para corrediças e
dobradiças) no próprio item na vista 3D. Os inspetores são:

| Item | O que dá para mudar |
|---|---|
| Pés e outras ferragens | Modelo, grupo, posição e rotação no mundo. Outras ferragens também têm as dimensões. |
| Corrediças | Modelo e comprimento, que valem na hora; altura e recuo; **Reajustar corrediças** |
| Dobradiças | Inspetor de montagem abaixo |
| Portas | Parte móvel, a peça em que ela é pendurada, quais dobradiças ela usa, limite de abertura e **Confirmar de novo**. O **+** adiciona outra dobradiça. |
| Modelos do catálogo | Dados, fonte e onde o modelo é usado. **Remover modelo** só funciona quando nada o usa. |

Valores digitados seguem a regra de sempre: **Enter** ou **Aplicar** salva,
**Esc** ou **Descartar** desfaz, e clicar em outro lugar nunca salva. Sair de
um item com valores não salvos pede confirmação.

Um aviso acompanha a instalação afetada, não outra dobradiça de nome
parecido. A remoção de peças relacionadas exige confirmação com os vínculos
afetados.

Selecionar uma instalação liga sua linha da árvore, o inspetor de montagem e
a referência projetada no visor. Eixo tracejado, marcas de caneco/calço e
conectores acompanham as coordenadas locais e a posição *apenas visual* da
porta aberta. Não são geometrias sólidas de usinagem; referências inválidas ou
sem evidência são avisadas, não tratadas como furos validados. O inspetor mantém
coordenadas Y independentes para porta e suporte, bordas, faces e pares K/R,
mesmo quando houver uma configuração rápida. O controle de ângulo e suas marcas
usam o limite efetivamente documentado no catálogo verificado do projeto; o
kit incluído atualmente permite 105°. O indicador no visor diz **Apenas
visualização**. **Sair da prévia** fecha a porta, e sair de Ferragens com
sucesso também restaura a posição fechada salva sem editar o projeto.
Formulários pendentes de relação ou instalação precisam ser resolvidos antes
de iniciar o movimento; referências inválidas desativam a prévia em vez de
inventar uma trajetória.

Adicione ao catálogo do projeto o kit FGVTN Click 3D Slow Reta / H=0 `51MX153DRV00100` com calço `52MX15FG11003D`; registre uma instalação por dobradiça física. Escolha painéis distintos para a porta e a montagem. Uma nova dobradiça começa na peça selecionada, e a lateral é escolhida para você: a peça mais próxima em ângulo reto com a porta, dando preferência às bordas longas da porta. A posição sugerida é o próximo ponto padrão livre (100 mm de cada ponta, depois entre elas). Com **Alinhar com o armário automaticamente** ligado (o padrão), o app calcula a borda da dobradiça na porta, a face do caneco, a borda frontal e a face da placa na lateral e a posição da placa a partir de onde as duas peças estão no modelo, para que a placa encontre o caneco na mesma altura. Desligue a opção, ou quando as peças não forem paralelas, para escolher as bordas (X-/X+ correm ao longo do Y da peça, Y-/Y+ ao longo do X), as faces e a posição da placa. As posições são medidas ao longo da borda da dobradiça, a partir da ponta mínima.

No inspetor da dobradiça, alterar **Posição** move o caneco e leva a placa junto. **Alinhar placa com o caneco** aparece quando a placa está fora de linha com o caneco (por exemplo, depois de mover uma peça), e **Distribuir dobradiças** coloca todas as dobradiças da porta a 100 mm de cada ponta e as demais igualmente espaçadas, em um passo de desfazer. O inspetor também informa quantas dobradiças uma porta desse tamanho costuma levar (2 até 900 mm, 3 até 1500 mm, 4 até 2000 mm, depois 5). Deslizar dobradiças ao longo da porta não exige revisar a relação de porta de novo.

Edições, exclusões e atualizações do catálogo podem ser desfeitas. Redimensionar o painel mantém as coordenadas e atualiza os avisos na lista.

A espessura documentada da porta é 15–22 mm. Os pares K (borda até a borda do caneco) / R (sobreposição) são 3/15, 4/16, 5/17, 6/18 mm. A prévia mostra caneco Ø35 mm com profundidade 11,3 mm, centro em K + 17,5 mm, e furos de referência do calço H=0 separados por 32 mm a 37 mm da borda frontal. Configurações, espessuras ou posições fora dos limites exibem avisos; são referências locais provisórias, não instruções de furação.

**Furação dos fixadores indisponível:** tipos de parafuso, diâmetros e profundidades dos furos-guia, posições dos parafusos do caneco, folgas seguras e quantidade de dobradiças não foram verificados. Confira a orientação e a instalação na marcenaria. Fonte: [Catálogo geral FGVTN](https://www.fgvtn.com.br/site/novopdf/Catalogo_Geral.pdf), p. impressa 23 (PDF p. 14), revisão de maio de 2025 documentada na [análise da fonte](hinge-source-review.md). O projeto guarda uma cópia do catálogo; atualize-a explicitamente para rever as instalações dependentes.
# Relações de portas

## Direção de abertura e projetos antigos

Ângulos positivos afastam a borda livre da face escolhida para o caneco.
A borda X da dobradiça e a face Z do caneco determinam a direção; portas
espelhadas não usam um único sinal global. A regra também vale para montagens
giradas e aninhadas, sem mover a peça fixa nem as posições fechadas salvas.

Relações antigas podem ter a direção anterior do eixo. Abrir o arquivo não
reescreve seus bytes nem altera o eixo armazenado. Uma relação afetada exibe
o aviso de revisão e não pode iniciar o movimento. Em **Ações da porta**,
escolha **Editar relação de porta**, confira peças, dobradiças e eixo proposto,
e confirme. Cancelar preserva a relação antiga; confirmar é uma única edição
reversível. Desfazer restaura o eixo antigo e a exigência de revisão. Relações
cuja direção já corresponde à regra continuam utilizáveis. A correção não
valida a trajetória da dobradiça nem a ausência de colisões.

Salvar e reabrir preserva exatamente as poses e os eixos em ponto flutuante;
uma relação válida e inalterada não exige revisão apenas por ser reaberta.
Versões anteriores podiam arredondar um eixo derivado ao ler JSON. Se esse valor
arredondado foi salvo depois, ele continua preservado na abertura: confira e
confirme explicitamente a relação conforme descrito acima, sem reparo silencioso
dos dados armazenados. Avisos reais da instalação continuam bloqueando o movimento.

Os controles flutuantes da porta inspecionada iniciam a prévia em 0° ou
restauram a posição **Fechada**, sem editar o projeto. O cartão inferior
ajusta o ângulo. Editar e excluir instalações continuam no inspetor e no menu
de contexto da linha. Expanda **Referências completas, pares e fonte** para
ver todos os dados; ferragens de referência e outras ações de objetos estão
na seção avançada.

Depois de instalar as dobradiças, escolha **Adicionar relação de porta**. Selecione uma peça móvel ou raiz de montagem, uma peça fixa de montagem e instalações existentes compatíveis. A prévia lista a subárvore móvel (inclusive puxadores aninhados), a peça fixa, o eixo na posição fechada e os avisos das instalações. Confirmar cria uma relação reversível sem mover o projeto fechado. Editar altera a seleção de dobradiças; excluir remove a relação em uma etapa de desfazer. Ciclos, autorreferência e duplicatas de raiz móvel ou dobradiça são rejeitados. Selecione uma peça ou montagem e use **Excluir peça / montagem selecionada** para conferir alocações, instalações e relações dependentes antes da exclusão da subárvore em uma etapa reversível. Cancelar não altera o projeto. Avisos após mudanças em peças/dobradiças exigem revisão; o eixo de referência não garante folgas nem serve como instrução de furação.

O PDF examinado tem SHA-256 `e8aafa4f3656a108e8e91dd1681685455a4bf4f644cf01d4ecfa3fd80bf44df2` (metadados modificados em 16/09/2026). O desenho mostra 48 mm entre os centros de fixação do caneco, **não** uma especificação completa de furação: não deduza diâmetro/profundidade dos pré-furos nem a escolha dos parafusos. Não há classificação de carga, quantidade recomendada de dobradiças, adequação estrutural, trajetória precisa do mecanismo oculto ou folga livre de colisões verificadas. Os avisos na tela e no plano continuam válidos em todos os ângulos. O PDF e as imagens do fabricante não acompanham o aplicativo; somente dimensões factuais atribuídas à fonte e anotações próprias. Referências inválidas são omitidas das instruções numéricas do plano sem bloquear cortes de madeira válidos.

## Pacotes de catálogo

Os dados de dobradiças vêm de **pacotes de catálogo**: um arquivo TOML por
fabricante, com os dados de cada dobradiça e a fonte de onde foram lidos.
**Adicionar dobradiça do catálogo…** abre o navegador de catálogos:

- **Pacotes** lista cada pacote com um estado: **Revisado** (incluído no
  aplicativo e conferido com as fichas citadas), **Dados do usuário** (seu
  próprio arquivo), **Rascunho** ou **Erros**. Um pacote com erros aparece
  com cada problema e sua posição no arquivo, mas não oferece nada para
  adicionar.
- **Dobradiça** e **Braço** escolhem a família e a variante Reta, Curva ou
  Alta. Os dados mostram os códigos da dobradiça e do calço, caneco, faixa
  de espessura da porta, altura do calço H, furação e recuo frontal do
  calço, ângulo de abertura e fonte.
- **Bancada de teste** roda as mesmas verificações de um projeto numa porta
  de exemplo: escolha um par K da tabela, informe a espessura da porta e da
  lateral (e E para embutida) e veja se cabe, onde ficam os centros do caneco
  e do calço, ou qual verificação falha.
- **Adicionar ao projeto** fixa a variante escolhida no projeto. O projeto
  guarda sua própria cópia: editar ou apagar o pacote depois nunca altera um
  projeto salvo. **Atualizar do catálogo** substitui um registro fixado de
  forma explícita (uma edição que pode ser desfeita) e reavalia as
  instalações.

Registros dos seus pacotes dão medidas completas. O cartão, o inspetor e o
PDF os marcam como **Dados do usuário** e citam o pacote; confira com a ficha
do fabricante antes de furar. Um registro cujos números não fecham entre si
(por exemplo, caneco mais fundo que a porta mais fina, ou tabela K fora de
ordem) não gera orientação numérica.

### Dobradiças Alta (porta embutida)

Uma porta embutida fica entre as laterais do móvel. A tabela dá a **folga F**
entre a borda da porta e a lateral para cada K, em vez do recobrimento R. O
diálogo da dobradiça pede **E**: a distância da borda frontal da lateral até
a face interna da porta (a espessura da porta quando ela fica rente à
frente). O calço fica no recuo frontal mais E (por exemplo 37 + 18 = 55 mm).
Um E menor que a espessura da porta gera aviso, porque a porta ficaria
saliente na frente. A prévia de movimento gira a porta embutida pela aresta
frontal externa.

### Como escrever seu pacote

Coloque arquivos `.toml` na sua pasta de catálogos (**Abrir pasta** no
navegador; no macOS `~/Library/Application Support/Plan My Cabinet/catalogs`)
ou use **Importar pacote…** e depois **Recarregar**. As medidas são em
milímetros; decimais como `9.8` são exatos. O arquivo incluído
`catalogs/fgvtn.toml` é um exemplo completo; o formato está descrito, campo a
campo, no guia em inglês (`hardware-en.md`).

O carregador informa todos os problemas de uma vez. Erros (o pacote não pode
ser usado) incluem campos desconhecidos, ids inválidos, códigos repetidos,
medidas mais finas que 1 µm, faixa de espessura invertida, caneco tão fundo
quanto a porta mais fina e valores de K que não crescem. Avisos (o pacote
continua utilizável) incluem passos irregulares de R/F, ângulo de abertura
ausente, pacote em rascunho e pacote do usuário que substitui um incluído com
o mesmo id. Um aviso pode ser aceito para uma dobradiça com `allow`, mas
somente com um motivo.

Para verificar um pacote sem abrir o aplicativo, rode
`plan-my-cabinet --check-catalog meu-pacote.toml`. Ele mostra cada problema
com sua posição no arquivo e termina com erro se houver erros.

# Corrediças

Um par de corrediças liga uma gaveta às duas laterais do móvel ao lado dela.
A gaveta é um grupo de peças (um conjunto): o modelo **Gavetas** cria um grupo
por gaveta; numa gaveta que você mesmo montou, agrupe as peças primeiro.

Selecione uma gaveta (ou uma das peças dela) e toque em **+** em **Gavetas e
corrediças**. As corrediças são adicionadas na hora e abrem no inspetor, onde
dá para mudar modelo, comprimento, altura e recuo. Sem nada selecionado, abre
a janela de corrediças: escolha a gaveta e o modelo. O aplicativo
encontra as laterais da caixa e as laterais do móvel ao lado, mede as folgas e
escolhe o maior comprimento que cabe. Ele mostra o código do produto e se a
corrediça cabe. Você pode escolher o comprimento, a altura na lateral da
gaveta (centralizada por padrão) e o recuo da frente do móvel (2 mm por
padrão).

As verificações são:

- **Folga lateral.** A folga entre cada lateral da caixa e a lateral do móvel
  precisa estar dentro da folga da corrediça. Na maioria é 12,7 mm, +0,5/−0,
  então a caixa é 25,4 mm mais estreita que o vão. A TT90 pede 19 ±0,3 mm.
- **Profundidade.** Recuo mais comprimento cabem na lateral do móvel, e o
  perfil da gaveta cabe na lateral da caixa. Corrediça de 500 mm vai em caixa
  de 500 mm.
- **Altura.** A corrediça cabe na lateral da caixa e na do móvel.
- **Alinhamento.** As duas corrediças ficam na mesma altura e profundidade.

A lista **Corrediças** no painel de Ferragens mostra cada gaveta com o código
do produto. O inspetor mostra as folgas e, para cada lado, as distâncias dos
furos a partir da borda da frente e a altura da linha de centro da corrediça.
As posições dos furos vêm da ficha do fabricante; confira antes de furar.

**Prévia** puxa a gaveta nas corrediças (um controle em milímetros, até o
curso da corrediça). É só visualização, como a prévia da porta.

O modelo **Gavetas** instala corrediças em todas as gavetas. Escolha o modelo
de corrediça na configuração (padrão FGVTN TT45 Slowmotion) ou **Nenhuma**.
Com corrediça, a folga lateral vem da corrediça e o campo de folga lateral é
ignorado.

O pacote `catalogs/fgvtn-slides.toml` traz estas corrediças FGVTN / TN de
abertura total com fechamento suave, das fichas do fabricante
(veja [a revisão](catalogs/fgvtn-slides-review.md)):

| Modelo | Comprimentos | Carga | Altura | Folga lateral |
|---|---|---|---|---|
| TT45 Slowmotion (0073.045500SX …) | 350–550 mm | 45 kg | 45 mm | 12,7 +0,5/−0 |
| TT44 Slowmotion (zincada, branca, preta) | 350–550 mm | 35 kg | 45 mm | 12,7 +0,5/−0 |
| TT35 Slowmotion | 250–550 mm | 25 kg | 35 mm | 12,7 +0,5/−0 |
| TN H45 Slow | 250–550 mm | 35 kg | 45 mm | 12,7 +0,5/−0 |
| TT90 Slow (carga pesada) | 450–600 mm | 90 kg | 52 mm | 19 ±0,3 |

O PDF da oficina lista as corrediças a comprar ("0073.045500SX … — 3 pares")
e, para cada gaveta, as distâncias dos furos nas duas peças de cada lado.

# Pés

Pés são ferragens de catálogo. Selecione o móvel e toque em **+** em **Pés**:
o pé fica embaixo dele, a 20 mm do canto, e abre no inspetor. Ali você muda o
modelo (a face de fixação fica no lugar), o grupo ao qual pertence e a posição
no mundo; também dá para arrastá-lo na vista 3D com **Mover peça**. A posição é o
canto da caixa do pé no chão; a face de fixação fica em cima. Pés não são
cortados do estoque e não levantam o móvel. Enquanto um pé passa do chão, o
inspetor oferece **Levantar <móvel> N mm**.

Os pés são desenhados com a forma do produto, para você ver como a peça vai
ficar: um cone de plástico, um pé cromado com chapa e sapata niveladora, um
quadro de tubo industrial. O pacote `catalogs/generic-feet.toml` traz medidas
típicas de produtos comuns. São **medidas genéricas de referência**, não a
ficha de um fabricante, e aparecem assim na tela e no PDF:

| Modelo | Medida |
|---|---|
| Pé plástico cônico 4 cm (preto, branco) | Ø50 → Ø30 × 40 mm, um parafuso |
| Pé quadrado cromado regulável 6/10/12/15/20 cm | chapa 60 × 60, tubo 32 × 32, sapata Ø38, regulagem 10 mm |
| Pé redondo cromado regulável 8/10 cm | chapa Ø60, tubo Ø32, sapata Ø38 |
| Pé de mesa industrial 75 × 50 cm | quadro fechado de tubo 30 × 30 |
| Pé de mesa industrial reforçado 75 × 60 cm | quadro fechado de tubo 50 × 30 |
| Pé de mesa industrial trapézio 71 × 50 cm | 500 em cima, 400 no chão |
| Pé de mesa reto 71 cm | tubo 40 × 40, chapa 100 × 100, regulagem 30 mm |

O PDF da oficina lista os pés a comprar com medida e acabamento.

## Como escrever modelos de corrediças e pés

Corrediças e pés vão em pacotes de catálogo como as dobradiças, nas tabelas
`[[drawer_slides]]` e `[[feet]]`. Um pacote pode misturar dobradiças,
corrediças e pés. O exemplo completo está no guia em inglês
([hardware-en.md](hardware-en.md#writing-slide-and-foot-models)).

Formas de pé:

- `tapered`: um cone ou pirâmide maciço. `top` e `bottom` são seções
  (`{ diameter = … }` ou `{ width = …, depth = … }`) e `height`.
- `post`: tubo com chapa em cima e sapata opcional: `tube`, `plate`,
  `plate_thickness`, `glide = { diameter, height }`, `height`.
- `frame`: quadro fechado de tubo: `top_width`, `bottom_width` (menor para
  trapézio, igual a `tube_width` para V), `tube_width`, `tube_depth`,
  `crossbar_height` e `glide` opcionais, `height`.

Pacotes com `review = { status = "generic" }` trazem medidas típicas sem
ficha de fabricante (não precisam de `[[sources]]`). Modelos criados por um
agente de IA com `save_to_catalog` são gravados em `user-models.toml` na sua
pasta de catálogos; use **Recarregar** para vê-los.
