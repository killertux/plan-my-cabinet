# Fita de borda

A fita de borda cobre o miolo aparente da borda de uma peça. No Plan My
Cabinet só peças de **MDF** e **MDP** levam fita: compensado, HDF, madeira
maciça e outros materiais ficam como estão, e o app nunca coloca fita neles.

## Fitas e materiais

- As **fitas de borda** ficam no painel do Projeto 3D, abaixo de Materiais.
  Cada uma tem um nome, como a marcenaria usa ("Fita Branca 1x22"), espessura,
  altura e cor. Projetos novos já vêm com as fitas branca e crua mais comuns.
  Toque numa fita para editá-la; ela só pode ser removida quando nada a usa. A
  lista mostra quantos metros de cada fita o projeto usa.
- **O tipo e a fita padrão de um material** ficam na janela do material.
  *Tipo* diz do que é a chapa (MDF, MDP, HDF, compensado, madeira maciça,
  outro); arquivos de antes da fita de borda tiram o tipo do nome do material.
  Só MDF e MDP oferecem *Fita de borda padrão*: a fita que o automático usa.

## Fita automática

Toda borda é **automática** até você mudar. Uma borda automática recebe a fita
padrão do material quando está **livre**, e fica sem fita quando outra peça
encosta nela: pelo menos metade da face da borda coberta, com folga de no
máximo 0,5 mm. Assim as bordas da frente das laterais, da base e das
prateleiras levam fita; as pontas de uma base entre as laterais não; a borda de
trás de uma lateral encostada no fundo não; portas e frentes de gaveta levam
fita em volta toda.

Portas e gavetas se movem, então as peças delas só encostam em peças que se
movem junto: uma frente de gaveta fechada não esconde as bordas do móvel atrás
dela.

A fita automática acompanha o projeto: mova ou redimensione uma peça e as
bordas se atualizam. Por isso os modelos de móvel em MDF branco já saem com
fita.

## Mudando a fita

Selecione uma peça no Projeto 3D. A seção **Fita de borda** do inspetor mostra
a peça como um retângulo com as quatro bordas:

- Bordas com fita aparecem na cor da fita. **A** marca uma borda automática;
  contorno tracejado é automático, contínuo é definido à mão. A borda da frente
  aparece como *frente*. Passe o mouse numa borda para ver por que ela tem (ou
  não tem) fita.
- **Toque numa borda** para colocar ou tirar a fita. Clique com o botão
  direito e escolha **Voltar ao automático** para desfazer sua escolha nessa
  borda.
- **Fita** escolhe a fita que as bordas novas recebem.
- **Automática · Nenhuma · Frente · Todas** definem as quatro bordas de uma vez.

Selecione várias peças para definir juntas: cada borda mostra em quantas das
peças selecionadas ela tem fita, e as predefinições valem para todas. Peças que
não levam fita são puladas e o app avisa.

Na vista 3D, as faces de borda com fita aparecem na cor da fita, com uma linha
fina ao longo delas. A ferramenta **Fita nas bordas** (na barra de
ferramentas) deixa você tocar nas bordas direto na vista: a borda sob o ponteiro
fica contornada, um toque inverte, e Alt-toque volta ao automático.

Cada mudança é um passo de desfazer. Trocar uma peça para um material que não
leva fita remove a fita dela no mesmo passo, e uma mensagem diz quantas bordas
perderam a fita.

## Medidas

As medidas das peças continuam sendo as medidas **finais**, com a fita. A
marcenaria desconta a espessura da fita no corte (o CorteCloud faz isso
sozinho).

## Nas saídas

- O PDF da oficina ganha uma coluna **Fita de borda** na lista de peças (por
  exemplo "C1 C2 L1 · Fita Branca 1x22": C1 e C2 são as bordas do comprimento,
  L1 e L2 as da largura) e uma tabela **Fita de borda** com os metros de cada
  fita, mais 10 % para comprar.
- O arquivo do CorteCloud manda cada fita no seu lado da peça.

Na área 3D, uma borda com fita fica na cor da fita, e uma borda sem fita
mostra o miolo cru da chapa (veja [Revestimento e aparência das chapas](coating-pt-BR.md)).
