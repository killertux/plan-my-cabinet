# Peças e materiais (português do Brasil)

## Área Projeto 3D e controles avançados da peça

Em Projeto 3D, a lista hierárquica mostra as peças reais e seus conjuntos pais;
selecionar uma linha destaca a mesma peça na vista 3D nativa. O inspetor mostra
material, espessura efetiva e sua origem, dimensões locais, fibras, pose,
estado da alocação e prévia do estoque. O painel flutuante de uma peça
selecionada oferece edição de comprimento/largura; controles avançados de
peças/materiais, espessura, hierarquia e posicionamento continuam disponíveis
no inspetor e nas ações dos objetos. O cabeçalho oferece navegação de projetos
e Salvar. Peças novas ou duplicadas são partes físicas independentes; uma peça
sem posição na chapa aparece **Sem alocação** e continua nos diagnósticos.
Consulte [Conjuntos e hierarquia](assembly-pt-BR.md) para posicionar e
[Estoque](stock-pt-BR.md) para operações de estoque/alocação.

## Criar e editar

Primeiro, use **Novo material** para informar nome e espessura positiva,
escolher a direção padrão das fibras e, opcionalmente, uma amostra de cor.
A cor é metadado visual, não altera o estoque nem propriedades de corte.
**Nova peça** pede nome, material,
comprimento local e largura local. Antes da confirmação, mostra material e
espessura efetiva. A previsão da primeira alocação viável é apenas uma prévia,
sem reservar estoque; a confirmação revalida material e estoque atuais. É
possível abrir Novo material do rascunho da peça e voltar ao rascunho intacto
ao cancelar esse formulário. A espessura vem do material selecionado;
não há terceiro campo de medida na criação da peça. A peça nova segue o padrão
do material; para compensado, a convenção inicial é ao longo do comprimento
local. Medidas ausentes, zero, negativas, não finitas ou irrepresentáveis não
podem ser confirmadas. Cancelar ou falhar não cria uma peça incompleta.

**Comprimento**, **Largura** e **Espessura** correspondem aos eixos locais X,
Y e Z da própria peça. Girar a peça ou o conjunto pai muda sua orientação no
espaço, mas não os nomes dessas medidas nem as dimensões da peça retangular
inicial. Por exemplo, uma lateral de 2300 × 600 × 18 mm continua com 2300 mm
de comprimento quando colocada na vertical. Use **Pose numérica** (inclusive
as predefinições canceláveis), posicionamento face a face ou Mover peça para
alterar a pose sem renomear esses eixos locais.

O menu **Fibras da peça** oferece **Seguir padrão do material**, **Ao longo
do comprimento**, **Ao longo da largura** e **Sem restrição**. As últimas
três opções são escolhas explícitas da peça, vinculadas aos eixos locais
mesmo após rotação; sobrevivem à mudança do padrão do material. A lista mostra
a **Direção efetiva das fibras (eixo local da peça)**. Mudar as fibras não
reposiciona silenciosamente uma alocação de estoque.

**Editar dimensões** permite escolher um eixo local e a **Referência do
redimensionamento**. **Manter face inicial** fixa a face de coordenada mínima
do eixo; **Manter face final**, a de coordenada máxima; **Manter centro** fixa
o ponto médio e move as duas faces igualmente. As referências são locais
mesmo dentro de um conjunto girado. O valor e a referência propostos aparecem
antes de **Confirmar**. Uma edição inválida mantém a medida anterior. Editar
a espessura aqui altera a espessura efetiva desta peça, não o padrão do
material nem a espessura medida de uma chapa de estoque.

**Atribuir material** mostra a espessura efetiva resultante e permite escolher
a referência para a espessura. **Editar material** mostra as peças afetadas e
exige optar por **Preservar peças existentes** ou **Aplicar a todas as peças
afetadas**. Preservar mantém a espessura efetiva e o estado das fibras das
peças existentes; novas atribuições usam o padrão revisado. Aplicar altera
explicitamente as peças afetadas com a referência escolhida. A espessura
medida do estoque nunca acompanha implicitamente a edição do padrão. A
compatibilidade exige identidade do material *e* espessuras efetivas iguais
entre peça e chapa. Conflitos resultantes são informados, não corrigidos por
realocação silenciosa.

**Duplicar peça** copia dimensões, material e fibras, mas cria uma peça física
com outro ID. Na interface atual, a cópia recebe deslocamento de 25 mm no eixo
X. Pode manter o mesmo nome exibido: diferencie as linhas pelo ID mostrado
abaixo de cada uma. Editar uma não altera a outra; a cópia não herda alocação
de estoque. Marque as caixas das linhas e use **Editar dimensões selecionadas**
para alterar uma dimensão local de várias peças. O diálogo mostra **Valores
diferentes** quando necessário, lista as peças e referências individuais e
confirma todas as alterações válidas juntas, ou nenhuma se houver alvo
inválido.

## Edições no inspetor e no painel flutuante

Em **Projeto 3D**, selecione a peça na estrutura ou na cena. Sem seleção, o
inspetor não sugere uma peça ativa; com vários objetos selecionados, mostra o
contexto real da seleção/lote em vez de escolher uma peça arbitrária. Peças
ocultas continuam selecionáveis na estrutura e ainda precisam de estoque.

Com uma peça selecionada, edite **Comprimento** ou **Largura** no inspetor ou
no painel flutuante. As duas superfícies mostram o mesmo texto pendente,
referência local do redimensionamento, validação e consentimento de
arredondamento; alternar entre elas não duplica a edição. Texto exibido
arredondado e não alterado preserva o valor exato gravado. Enter ou **Aplicar**
aceita um rascunho válido em uma única edição reversível; Escape ou
**Descartar** cancela. Ao tentar mudar de alvo/área com entrada inválida ou
arredondamento não confirmado, é preciso escolher Aplicar (indisponível até
ser válido), Descartar ou Permanecer; recolher o inspetor mantém o rascunho.
Alterar o texto limpa o consentimento anterior; perder o foco não confirma.
A unidade de exibição e o idioma de entrada são capturados na primeira edição
do texto, de modo que trocar unidade/idioma durante o rascunho não o reinterpreta.
Um sufixo explícito continua prevalecendo. Os comandos avançados **Editar
dimensões**, inclusive espessura com referência e **Editar dimensões
selecionadas** para várias peças, permanecem disponíveis; a espessura no
painel flutuante é o *valor efetivo da peça* apenas para leitura, que pode
diferir do padrão atual do material.

Para uma peça alocada, a miniatura clicável do inspetor mostra a chapa real,
margens de refilo e posições das peças, destacando a peça selecionada. Abra-a para ir
à alocação da mesma peça no Plano de corte. Uma peça sem alocação leva ao seu
problema de alocação. Navegar não reposiciona a peça nem seleciona todas as
peças da chapa; rascunhos pendentes recebem a mesma decisão
Aplicar/Descartar/Permanecer. A miniatura é um esboço de estoque, não uma
comprovação independente da viabilidade dos cortes.

## Passo a passo só com teclado pelos controles avançados

Ao abrir um diálogo, o foco vai para o primeiro controle. `Tab` (ou
`Shift+Tab` para voltar) permanece no diálogo até fechá-lo. `Espaço` aciona botões e caixas com
foco. Não há atalhos específicos para criar, duplicar ou editar peças. Os
valores abaixo são exatos em milímetros e não pedem arredondamento; digite a
unidade para evitar ambiguidade. Para substituir o texto inteiro no campo com
foco, use `Command+A` no macOS ou `Ctrl+A` no Linux.

Com um projeto aberto, `⌘/Ctrl+1–5` alterna entre áreas; não cria objetos.
Use `⌘/Ctrl+K` e busque **Novo material** ou **Nova peça** para abrir os formulários.

1. Execute `cargo run --locked`. Na tela inicial, use `Tab` para chegar a
   **Novo projeto** e pressione `Espaço`. Em **Projeto 3D**, vá a **Novo material**
   e pressione `Espaço`; se os controles à esquerda estiverem recolhidos,
   reabra **Projeto 3D** pelo cabeçalho primeiro.
   **Nome do material** já está em foco: digite `Compensado 18`;
   vá a **Espessura** e digite `18 mm`. Deixe **Direção padrão das fibras**
   em **Ao longo do comprimento**. Pressione `Return` para criar. `Escape`
   cancelaria.
2. Vá a **Nova peça** e pressione `Espaço`; **Nome da peça** já está em foco: digite
   `Lateral`. O primeiro material já está selecionado. Em **Comprimento**,
   digite `2300 mm`; em **Largura**, `600 mm`. Confira material e espessura
   efetivos de 18 mm e pressione `Return`. `Lateral` aparece em **Estrutura**.
   Nos controles à esquerda, expanda a seção inferior **Visualização avançada
   e idioma** que contém as ações das peças para ver seu ID, `2300 × 600 × 18`
   e **Sem alocação**, além dos controles de duplicação/edição usados abaixo.
3. Vá ao botão **Duplicar peça** da linha `Lateral` e pressione `Espaço`.
   Agora existem duas linhas `Lateral` com IDs distintos. Localize a segunda
   por posição e ID: também mede `2300 × 600 × 18` e está **Sem alocação**.
4. Vá a **Editar dimensões** da *segunda* linha e pressione `Espaço`. O
   **Eixo local** inicia em **Comprimento** e a **Referência do
   redimensionamento** em **Manter centro**. Vá ao campo **Comprimento**,
   pressione `Command+A`/`Ctrl+A` e digite `2200 mm`. Vá a **Confirmar** e
   pressione `Espaço`. A primeira linha continua `2300 × 600 × 18`, enquanto
   a cópia indica `2200 × 600 × 18`.
5. Vá a **Editar dimensões** da *primeira* linha e pressione `Espaço`;
   substitua **Comprimento** por `2400 mm` da mesma maneira e acione
   **Confirmar** com `Espaço`. As linhas agora indicam `2400 × 600 × 18` e
   `2200 × 600 × 18`: cada peça foi editada independentemente. `Escape`
   cancela qualquer um dos diálogos de edição sem confirmar.

Para uma entrada não exata como `1/64 in`, o campo apresenta 0,397 mm como
arredondamento proposto e uma caixa de confirmação. Dê foco à caixa e
pressione `Espaço` para consentir antes de confirmar; alterar o texto limpa
esse consentimento. Apenas sair do campo com `Tab` nunca aceita o
arredondamento. Consulte [Entrada de medidas](input-pt-BR.md) para unidades e
sintaxe decimal.
