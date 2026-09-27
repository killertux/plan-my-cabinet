# Entrada de medidas (Português brasileiro)

**Configurações** reúne Corte, Grade e unidades, Custos e moeda, Geral,
Atalhos e Sobre. **Concluído** no rodapé ou Escape fecha as configurações sem
aceitar um editor filho pendente. Preferências do aplicativo são salvas
automaticamente; grade, espessura do corte e custos continuam sendo edições
explícitas do projeto. Em escala ampliada, o conteúdo rola sem perder as
seções ou Concluído. Enter/Escape em um menu filho afeta primeiro esse menu.

As dimensões de fabricação são armazenadas com resolução de 0,001 mm. Você
pode digitar decimais sem separador de milhar usando vírgula ou ponto,
independentemente do idioma da interface. `1,234 mm` significa **1,234 mm**
(um milímetro e 234 milésimos), não 1.234 mm (mil duzentos e trinta e quatro
milímetros). Não misture ponto e vírgula nem separe grupos de algarismos com
espaços; use `1200 mm` ou `1,2 m`. Sem sufixo, vale a unidade exibida no campo.

Sufixos aceitos: `mm`, `cm`, `m`, `in`, `ft`, além de `"` para polegadas e `'`
para pés. Frações em polegadas como `3/4 in` e `1 1/2 in` são aceitas; `3/0 in`
não é. A prévia mostra o valor convertido ou o arredondamento proposto. Esse
arredondamento exige confirmação explícita. Apenas entrar e sair de um campo
não altera a medida armazenada por causa do texto exibido.

Trocar a unidade de exibição ou o idioma não redimensiona peças nem converte
preços. O projeto usa uma única moeda; trocá-la exige preços substitutos ou
uma confirmação de troca do rótulo sem conversão cambial.

## Rascunhos no inspetor e na barra da seleção

Para uma única peça selecionada, o inspetor e a barra inferior da seleção
editam **o mesmo** comprimento e a mesma largura locais pendentes, com âncora,
validação e consentimento de arredondamento compartilhados. Corrigir o texto
em uma das superfícies atualiza a outra; os dois campos são confirmados juntos
em uma única edição reversível. O inspetor também oferece rascunhos numéricos
de posição (X/Y/Z) e rotação (X°/Y°/Z°), no referencial Mundo ou Local do pai.
As posições seguem as mesmas regras de medida e arredondamento; as rotações
são em graus. São propostas até a confirmação. O resumo compacto de espessura
mostra a espessura efetiva da peça; use o editor avançado de dimensões no
inspetor para alterar a espessura com âncora e validação. A rota existente de
Colocar face a face continua disponível.

Com um campo em foco, **Enter** ou **Aplicar** confirma comprimento/largura
válidos; **Escape** ou **Descartar** cancela a edição. Para a pose no inspetor,
use **Enter** ou **Aceitar** para confirmar, e **Escape** ou **Cancelar prévia**
para restaurar a pose anterior. Texto inválido ou arredondamento ainda não
autorizado impede a confirmação. Por exemplo, `1/64 in` equivale a cerca de
0,396875 mm: a proposta de 0,397 mm exige consentimento explícito antes de
Aplicar/Aceitar. Alterar o texto depois disso apaga o consentimento. Mudar o
foco, sem editar, não confirma: uma exibição arredondada de 12,35 mm para um
valor exato de 12,345 mm preserva 12,345 mm e não acrescenta um passo ao
histórico. Cancelar também preserva o projeto e suas alocações.

A primeira edição em cada campo de medida fixa a unidade e o idioma de
entrada daquele rascunho. Se você digitar `1,5` sem sufixo em milímetros com a
interface em pt-BR e depois mudar para centímetros e inglês, o texto pendente
ainda significa **1,5 mm**; não é reformatado nem confirmado. Um sufixo
explícito como `1/64 in` prevalece sobre a unidade capturada. A validação, a
medida proposta e o consentimento ainda aplicável ao texto intacto sobrevivem
à troca. Outras edições no mesmo campo continuam usando o contexto capturado,
mas apagam o consentimento. Campos não editados podem ser reformatados a partir
dos valores exatos armazenados; novos rascunhos usam as novas preferências.
Trocar unidade ou idioma, por si só, não cria edição nem passo de desfazer.

Sair da área de trabalho, trocar a peça selecionada/alvo do inspetor ou iniciar
uma ação incompatível durante a edição abre uma escolha: **Aplicar / Descartar
/ Permanecer** para campos de dimensão e **Aceitar / Cancelar prévia /
Permanecer** para uma prévia de pose ou reparo. Aplicar/Aceitar valida e então
segue ao destino; Descartar/Cancelar prévia segue sem a edição; Permanecer
conserva o rascunho e o local atual. Entrada inválida ou falta de consentimento bloqueia a
confirmação: permaneça para corrigir ou descarte/cancele para sair. Fechar ou
recolher o inspetor conserva texto, erros e consentimento para quando ele for
reaberto; não confirma nem cancela a edição.

## Janelas de criação, posicionamento e redimensionamento em lote

**Nova peça** e **Novo material** usam formulários centralizados e isolados.
As amostras de cor do material só alteram a aparência; a previsão da primeira
alocação viável da peça não reserva estoque nem modifica o projeto. É possível
revisar campos várias vezes e cancelar sem criar peça, alocação ou material.
Ao confirmar, o material e o estoque atuais são revalidados: uma previsão
anterior à alteração do estoque não autoriza posição desatualizada. Abrir
Novo material de um rascunho de peça e cancelá-lo retorna ao formulário intacto.
Uma medida inexata requer consentimento explícito ao arredondamento; editar a
proposta apaga o consentimento.

**Pose numérica** e **Posicionar face a face** mostram prévias sem modificar o
projeto. A primeira oferece referencial Mundo/Local do pai e posição/rotação
XYZ; a segunda identifica peças e faces, alinhamento/deslocamento no plano e
afastamento externo. **Aplicar** ou **Posicionar** no rodapé grava uma pose em
uma única edição reversível; **Cancelar** ou Escape descarta a prévia.
**Redimensionar N peças** distingue valores atuais diferentes, mostra o valor
anterior/posterior e a âncora de cada peça e aplica a mudança validada em uma
única edição reversível. Entrada inválida ou arredondamento não consentido não
pode ser aceita. A janela modal impede que arrastes, atalhos de projeto ou
rolagem atinjam a cena atrás do formulário; Enter não envia o formulário se
uma lista suspensa consumir a tecla, e Escape fecha primeiro essa lista ou a
camada de edição ativa.

## Atalhos das áreas de trabalho e da busca de comandos

Use **⌘** no macOS ou **Ctrl** nos demais sistemas de desktop compatíveis para
os atalhos abaixo. Com um projeto aberto, **⌘/Ctrl+1–5** alterna, respectivamente,
entre Projeto 3D, Estoque, Plano de corte, Ferragens e Entrega. **⌘/Ctrl+K** abre
Buscar comandos na área de trabalho. O cabeçalho também oferece Buscar comandos,
Salvar, desfazer/refazer e Exportar; Exportar abre Entrega para revisar a
preparação da saída.

Em Buscar comandos, digite para encontrar ações ou peças, materiais,
chapas de estoque (inclusive códigos), instalações de dobradiças e relações de
portas por nome ou ID. Uma busca vazia lista as ações disponíveis. Use as
**setas para cima/baixo** para selecionar um resultado, **Enter** para ativá-lo
ou **Escape** para fechar a busca e devolver o foco. Um resultado desabilitado
informa o motivo; selecionar um item navega até o alvo identificado. Edições
pendentes podem exigir a escolha Aplicar/Descartar/Permanecer ou Aceitar/Cancelar
prévia/Permanecer descrita acima.

**⌘/Ctrl+S** salva o projeto, **⌘/Ctrl+Z** desfaz e **⌘/Ctrl+Shift+Z**
refaz quando essas ações estão disponíveis. Os atalhos de projeto e de área de
trabalho não interferem no texto em foco, em janelas modais nem em menus
suspensos. Buscar comandos também não abre durante outra janela modal ou
operação de arquivo. Para um campo de dimensão ou pose em foco, use o tratamento
próprio de Enter/Escape descrito acima.
