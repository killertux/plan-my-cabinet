# Peças e materiais (português do Brasil)

## Interface atual

O painel esquerdo oferece **Novo material**, **Nova peça**, uma lista de
materiais e outra de peças. A visualização 3D central ainda mostra uma cena
fixa de teste: não exibe, seleciona nem gira as peças da lista. A lista é a
representação atual das peças criadas. Ainda não há interface para cadastrar
chapas ou posicionar peças nelas; peças novas e duplicadas aparecem **Sem
alocação**. A alocação de estoque virá depois. A interface atual também não
oferece Abrir/Salvar projeto.

## Criar e editar

Primeiro, use **Novo material** para informar nome e espessura positiva e
escolher a direção padrão das fibras. **Nova peça** pede nome, material,
comprimento local e largura local. Antes da confirmação, mostra material e
espessura efetiva. A espessura vem do material selecionado;
não há terceiro campo de medida na criação da peça. A peça nova segue o padrão
do material; para compensado, a convenção inicial é ao longo do comprimento
local. Medidas ausentes, zero, negativas, não finitas ou irrepresentáveis não
podem ser confirmadas. Cancelar ou falhar não cria uma peça incompleta.

**Comprimento**, **Largura** e **Espessura** correspondem aos eixos locais X,
Y e Z da própria peça. Girar a peça ou o conjunto pai muda sua orientação no
espaço, mas não os nomes dessas medidas nem as dimensões da peça retangular
inicial. Por exemplo, uma lateral de 2300 × 600 × 18 mm continua com 2300 mm
de comprimento quando colocada na vertical. A interface atual ainda não
oferece controle de rotação.

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

## Passo a passo só com teclado na interface atual

Ao abrir um diálogo, o foco vai para o primeiro controle. `Tab` (ou
`Shift+Tab` para voltar) permanece no diálogo até fechá-lo. `Espaço` aciona botões e caixas com
foco. Não há atalhos específicos para criar, duplicar ou editar peças. Os
valores abaixo são exatos em milímetros e não pedem arredondamento; digite a
unidade para evitar ambiguidade. Para substituir o texto inteiro no campo com
foco, use `Command+A` no macOS ou `Ctrl+A` no Linux.

1. Execute `cargo run --locked`. Com `Tab`, vá a **Novo material** e pressione
   `Espaço`. **Nome do material** já está em foco: digite `Compensado 18`;
   vá a **Espessura** e digite `18 mm`. Deixe **Direção padrão das fibras**
   em **Ao longo do comprimento**. Pressione `Return` para criar. `Escape`
   cancelaria.
2. Vá a **Nova peça** e pressione `Espaço`; **Nome da peça** já está em foco: digite
   `Lateral`. O primeiro material já está selecionado. Em **Comprimento**,
   digite `2300 mm`; em **Largura**, `600 mm`. Confira material e espessura
   efetivos de 18 mm e pressione `Return`. A lista **Peças do projeto** agora
   mostra `Lateral`, `2300 × 600 × 18` e **Sem alocação**.
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
