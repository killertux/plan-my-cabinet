# Navegação da visualização 3D

A visualização mostra as placas nas posições da montagem. X (comprimento) é
vermelho, Y (largura) verde e Z (espessura) azul. Clique em uma face visível
ou no nome da peça na lista à esquerda para selecioná-la; segure Shift ou
Command (macOS) / Ctrl (Linux) ao clicar para adicionar ou remover peças.
Clique no espaço vazio da cena para limpar a seleção. A peça ativa aparece em
âmbar e com `*` na lista; outras peças selecionadas ficam em ciano. Não é
possível clicar através de uma peça próxima para selecionar outra encoberta;
use a lista nesse caso. **Enquadrar seleção/cena** ajusta a câmera às
placas selecionadas ou a todas, se não houver seleção. Em um projeto vazio,
volta à origem. Escolha Isométrica, Frontal, Direita ou Superior e Perspectiva
ou Ortográfica nos controles acima da visualização. Mover a câmera não altera
o projeto.
Os botões Esq/Dir de Órbita/Deslocar e os botões Zoom +/- são alternativas aos
gestos de arrastar.

- Arraste com o botão principal (clique e arraste com um dedo no trackpad) para orbitar.
- Arraste com o botão secundário (clique e arraste com dois dedos no trackpad)
  ou segure Shift e arraste com o botão principal para deslocar a vista.
- Role verticalmente ou faça o gesto de pinça sobre a vista para aproximar ou afastar.
- Clique na vista para dar foco; as setas orbitam, Shift+setas deslocam a vista
  e `+`/`-` aproximam/afastam. Tab também pode focar a vista. Os atalhos da
  cena não funcionam com um diálogo ou menu aberto, nem com foco em outro
  controle ou campo de texto.

O modo padrão **Navegar** mantém a órbita com o arraste principal. Escolha
**Mover peça** acima da vista para arrastar a peça ativa selecionada (comece
sobre sua superfície visível). A posição é pré-visualizada num plano voltado
para a câmera sem alterar o projeto. Uma face visível próxima pode atrair a
peça: as faces de origem em ciano e destino em rosa e o aviso indicam a
posição proposta. Segure **Alt** durante o arraste para ignorar o encaixe e
posicionar livremente. Solte para aceitar um único movimento reversível;
pressione **Escape** durante o arraste para cancelar e restaurar a posição e
seleção originais. O arraste secundário ainda desloca a vista, e voltar a
Navegar restaura a órbita principal. Arrastar no espaço vazio em Mover peça
não move nenhuma peça.
Os encaixes só auxiliam a colocação: a pose gravada é independente do destino.
Mover depois a peça de destino não leva a peça colocada junto, a menos que
ambas pertençam a um conjunto movido. Nenhum encaixe cria dobradiça ou junta mecânica.

Na linha da peça, **Pose numérica** abre uma prévia com o referencial
explícito **Local do pai** ou **Mundo**. A posição usa mm (sufixos de unidade são
aceitos); a rotação usa graus em X, Y e Z. Coordenadas derivadas abaixo de
0,001 mm que não foram editadas permanecem exatas, sem aplicar o texto
arredondado da interface. **Posicionar face a face** permite escolher faces
de origem e destino, alinhamento inicial/central/final no plano, deslocamentos
e afastamento externo. Ciano e rosa destacam as faces propostas.
**Confirmar** grava uma única posição, sem vínculo permanente; **Cancelar**
ou Escape restaura a posição inicial.
Use Mundo para posições absolutas e Local do pai para coordenadas dentro de
um conjunto; rotações são graus XYZ. O alinhamento escolhe início/centro/fim
em cada eixo do plano, os deslocamentos seguem os eixos do plano da face-alvo
e o afastamento externo segue sua normal. Veja as etapas peça a peça e a
medição de corpo versus total em [Conjuntos e hierarquia](assembly-pt-BR.md).
# Grade XY do projeto

A grade da vista fica no plano XY global, em Z = 0, com origem no zero global. O espaçamento local do projeto começa em 10 mm. Use **Editar espaçamento da grade** na barra lateral para informar um valor positivo de até 1.000.000 mm; escolha mm ou in para entradas sem sufixo, ou digite mm, cm, m, in ou ft explicitamente (inclusive frações de polegada). Valores além da precisão de 0,001 mm exigem confirmação do resultado arredondado. Cancelar, inserir um valor inválido ou apenas focar o campo não altera o projeto. A grade visível fica menos densa em vistas afastadas, mas o encaixe sempre usa o espaçamento exato salvo.

Em **Mover peça**, arraste uma peça selecionada para perto de uma interseção da grade XY para visualizar a origem global X/Y encaixada; Z e rotação são preservados, inclusive sob um conjunto girado. O encaixe em face visível tem prioridade sobre o da grade. O status identifica a face-alvo ou **Grade XY**; segure Alt para ignorar ambos. A grade só atrai após o início do movimento e quando o candidato está próximo na tela, evitando saltos no início. Solte para aceitar uma única edição de pose reversível ou pressione Esc para cancelar. O encaixe não cria vínculo permanente; alterar o espaçamento não reposiciona peças existentes. O espaçamento é salvo no projeto e recuperado ao reabri-lo.
