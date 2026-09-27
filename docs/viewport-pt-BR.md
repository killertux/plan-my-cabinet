# Navegação da visualização 3D

Configurações → Geral controla dicas de navegação, inversão do zoom pela
rolagem, cor dos materiais e escala da interface (90/100/115/130%). Em Grade e
unidades, trocar a unidade exibida não move peças nem reinterpreta um campo
sem sufixo já editado: ele mantém a unidade e o idioma da primeira edição.
Sufixos explícitos prevalecem. Campos intactos são reformatados a partir da
medida exata salva; um rascunho arredondado ainda exige confirmação.

Abra **Projeto 3D** na barra de áreas. Os controles de câmera e ferramenta
ficam acima da cena central; em larguras menores, **Opções de vista** reúne
**Enquadrar seleção**, projeções e posições predefinidas; em larguras muito
pequenas, também reúne as ferramentas. O menu de encaixe controla separadamente
faces e grade, informa o espaçamento salvo e explica quando a grade exibida
fica menos densa ao afastar a câmera. O texto de ajuda sobre a cena é abreviado
em largura pequena; passe o ponteiro para ler as instruções completas de
teclado/trackpad. A câmera é um estado de visualização da sessão, não uma
edição do projeto. Quando os painéis laterais são recolhidos, as gavetas do
cabeçalho continuam acessíveis sem descartar o rascunho de dimensões do
inspetor ou do painel flutuante.

A visualização mostra as placas nas posições da montagem. X (comprimento) é
vermelho, Y (largura) verde e Z (espessura) azul. Clique em uma face visível
ou no nome da peça na lista à esquerda para selecioná-la; segure Shift ou
Command (macOS) / Ctrl (Linux) ao clicar para adicionar ou remover peças.
Clique no espaço vazio da cena para limpar a seleção. A peça ativa aparece em
âmbar e sua linha em Estrutura fica destacada; outras peças selecionadas ficam em ciano. Não é
possível clicar através de uma peça próxima para selecionar outra encoberta;
use a lista nesse caso. **Enquadrar seleção** ajusta a câmera à geometria
visível selecionada ou à cena visível quando nada está selecionado. Uma seleção
sem geometria visível enquadrável desabilita essa ação; limpe a seleção para
enquadrar a cena. Sem seleção e com a cena vazia, a câmera volta à origem.
Escolha **Iso**, **Frontal**, **Direita** ou **Superior** e **Perspectiva** ou
**Ortográfica** nos controles acima da visualização ou em **Opções de vista**.
Mover a câmera não altera o projeto.

- Arraste com o botão principal (clique e arraste com um dedo no trackpad) para orbitar.
- Arraste com o botão secundário (clique e arraste com dois dedos no trackpad)
  ou segure Shift e arraste com o botão principal para deslocar a vista.
- Role verticalmente ou faça o gesto de pinça sobre a vista para aproximar ou afastar.
- Clique na vista para dar foco; as setas orbitam, Shift+setas deslocam a vista
  e `+`/`-` aproximam/afastam; `F`, sem modificadores, aciona **Enquadrar seleção**
  quando disponível. Tab também pode focar a vista. Os atalhos da
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
e afastamento externo. Ciano e rosa destacam as faces propostas. **Aplicar**
(pose numérica) ou **Posicionar** (face a face) grava uma única posição, sem
vínculo permanente; **Cancelar** ou Escape restaura a posição inicial.
Use Mundo para posições absolutas e Local do pai para coordenadas dentro de
um conjunto; rotações são graus XYZ. O alinhamento escolhe início/centro/fim
em cada eixo do plano, os deslocamentos seguem os eixos do plano da face-alvo
e o afastamento externo segue sua normal. A janela centralizada apresenta o
resultado provisório sem gravá-lo até usar a ação nomeada no rodapé. Veja as
etapas peça a peça e a
medição de corpo versus total em [Conjuntos e hierarquia](assembly-pt-BR.md).
## Medir e dimensões projetadas da peça

Selecione uma única peça visível para ver **Comprimento** e **Largura** locais projetados nas arestas X e Y reais. Os rótulos acompanham a projeção da câmera e a pose da peça; descrevem a peça física segundo seus eixos locais, não a distância em pixels nem a caixa envolvente do conjunto. Peças ocultas e seleções múltiplas não exibem esse par de uma peça única. Inspetor e painel flutuante da seleção usam o mesmo rascunho pendente de Comprimento/Largura; os rótulos da vista são apenas informativos.

Escolha **Medir** acima da vista para mostrar X × Y × Z da caixa envolvente das peças ou conjuntos selecionados (e de ferragens compatíveis em Total), sem editar. Nos controles de medição, escolha **Somente corpo · peças de madeira** ou **Total · peças + ferragens selecionadas** e **Referencial** Mundo ou uma peça/conjunto. O resultado sobreposto identifica escopo, referencial e unidade de exibição; as escolhas permanecem ao trocar de ferramenta. Descendentes ocultos selecionados ainda contam; ferragens sem dimensões tornam Total indisponível, sem sugerir um resultado completo. Medir não altera poses, dimensões de fabricação, alocações nem histórico de desfazer. O inspetor também mostra a medida. Veja a convenção dos referenciais/predefinições e um exemplo em [Conjuntos e hierarquia](assembly-pt-BR.md).

## Grade XY do projeto

A grade da vista fica no plano XY global, em Z = 0, com origem no zero global. O espaçamento local do projeto começa em 10 mm. Use **Editar espaçamento da grade** em Configurações → Grade e unidades ou nos controles de Projeto 3D para informar um valor positivo de até 1.000.000 mm; escolha mm ou in para entradas sem sufixo, ou digite mm, cm, m, in ou ft explicitamente (inclusive frações de polegada). Valores além da precisão de 0,001 mm exigem confirmação do resultado arredondado. Cancelar, inserir um valor inválido ou apenas focar o campo não altera o projeto. A grade visível fica menos densa em vistas afastadas, mas o encaixe sempre usa o espaçamento exato salvo.

Em **Mover peça**, arraste uma peça selecionada para perto de uma interseção da grade XY para visualizar a origem global X/Y encaixada; Z e rotação são preservados, inclusive sob um conjunto girado. O encaixe em face visível tem prioridade sobre o da grade. O status identifica a face-alvo ou **Grade XY**; segure Alt para ignorar ambos. A grade só atrai após o início do movimento e quando o candidato está próximo na tela, evitando saltos no início. Solte para aceitar uma única edição de pose reversível ou pressione Esc para cancelar. O encaixe não cria vínculo permanente; alterar o espaçamento não reposiciona peças existentes. O espaçamento é salvo no projeto e recuperado ao reabri-lo.
