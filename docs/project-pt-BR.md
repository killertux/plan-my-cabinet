# Arquivos de projeto e recuperação (português do Brasil)
Configurações → Geral guarda idioma da interface, escala, dicas, inversão do
zoom e coloração por material neste computador, fora do projeto portátil.
**Concluído** ou Escape fecha as configurações; nenhuma dessas ações salva o
projeto ou aceita um rascunho de medida. Mudanças em corte/grade/custos usam
comandos validados e reversíveis, enquanto unidade de exibição e idioma não
alteram revisão, histórico de desfazer nem atualidade da fabricação.

## Disponibilidade

O aplicativo oferece Novo projeto, Abrir projeto, Salvar e Salvar como, com
indicador de alterações não salvas ao lado do nome/caminho. As janelas de
seleção nativas são assíncronas. Novo, Abrir e fechar pedem para salvar,
descartar ou cancelar alterações não salvas. Abrir valida o arquivo escolhido
antes de pedir para descartar o trabalho atual. Salvar como exige confirmação
antes de substituir um destino existente. O histórico de desfazer/refazer
recomeça para cada projeto aberto ou criado.

## Edições pendentes, navegação e atalhos

### Começar com um modelo de armário

A tela inicial oferece **Base**, **Parede** e **Gavetas** mesmo na primeira
abertura, sem projeto ou biblioteca de materiais. Cada opção inicia a
configuração provisória de um novo projeto: escolha nome, moeda e unidades de
entrada; crie materiais com espessura, veio e cor opcional; associe-os aos
componentes. Papéis compatíveis podem usar o mesmo material. Cancelar a
criação de um material preserva a configuração do modelo. A revisão mostra
premissas construtivas, dimensões das peças, referências de profundidade,
folgas e previsão de alocação antes de **Gerar projeto**. Geometria inválida
ou arredondamento não confirmado impede a geração.

A **Base** contém laterais de altura total, fundo e travessas superiores; o
**Parede** contém tampo, fundo, prateleira e painel traseiro sobreposto.
**Gavetas** contém carcaça, laterais/frentes/traseiras/fundos das caixas e
frentes externas independentes. Informe a quantidade, profundidade das caixas,
folgas laterais e traseiras, folga vertical, recuos e intervalos das frentes.
As peças geradas são editáveis individualmente, sem vínculos paramétricos
permanentes. O modelo não seleciona corrediças, não fornece instruções de
usinagem nem certifica carga ou encaixe mecânico.

A configuração não altera o projeto aberto. **Cancelar configuração** volta à
tela inicial sem criar projeto ou entrada recente. Antes de gerar, resolva
edições pendentes e alterações não salvas do projeto anterior; falha ou
cancelamento ao salvar preserva tanto o projeto quanto a configuração. Após
gerar, Projeto 3D abre o novo projeto não salvo com o conjunto selecionado.
Materiais e peças são uma única transação reversível. Sem estoque declarado,
as peças ficam sem alocação: use **Abrir Estoque** para cadastrar chapas
físicas medidas. Só um salvamento explícito e bem-sucedido registra o projeto
nos recentes.

O inspetor de Projeto 3D e a barra da peça selecionada compartilham um rascunho
temporário de comprimento/largura; os campos numéricos de pose no inspetor
têm proposta própria. Enter ou Aplicar confirma dimensões válidas em uma só
edição reversível; Enter ou Aceitar confirma a pose válida. Escape ou Descartar
cancela as dimensões; Escape ou Cancelar prévia cancela a pose. A mudança de
foco não salva nem confirma rascunhos. Consulte [Entrada de medidas](input-pt-BR.md)
para unidade/idioma capturados, valores exatos e consentimento de
arredondamento. O editor avançado de espessura e a ação Colocar face a face
continuam disponíveis separadamente.

Trocar de área de trabalho, mudar o alvo do inspetor/seleção ou iniciar uma
ação incompatível durante uma edição oferece **Aplicar / Descartar /
Permanecer** para campos ou **Aceitar / Cancelar prévia / Permanecer** para uma
prévia de pose/reparo. Não é possível aceitar entrada inválida, arredondamento não
autorizado ou reparo inválido; uma falha de validação mantém o rascunho e o
local para correção. Permanecer mantém ambos; Descartar/Cancelar segue sem
alterar o estado confirmado anterior ao rascunho. O destino é revalidado após
a confirmação: um alvo removido ou projeto/revisão desatualizado não é
selecionado silenciosamente. Recolher e reabrir um painel conserva o rascunho
e sua validação sem exigir decisão de navegação. Trocas de área de trabalho
preservam o contexto da sessão, como seleção, estado dos painéis, filtros e
rolagem, sem criar edições no modelo.

Salvar e a substituição de projeto primeiro resolvem uma edição pendente;
Aplicar com sucesso pode então prosseguir ao salvamento ou à decisão habitual
sobre alterações não salvas ao criar/abrir/fechar. Salvar não inclui textos ou
prévias não confirmados. Novo/Abrir/fechar ainda consultam sobre alterações
**confirmadas** não salvas; Abrir valida o arquivo preparado antes de trocar
o projeto atual. Cancelar a seleção de arquivo ou falhar ao salvar não
substitui o trabalho atual. Quando o projeto é efetivamente substituído,
rascunhos, navegação, seleção e estado de visualização da sessão anterior são
limpos: não são dados portáteis do `.pmcab` nem restaurados pelo salvamento de
recuperação. Uma edição confirmada pode ser desfeita na sessão atual;
descartar um rascunho não acrescenta passo ao histórico.

**Command-S** (Control-S onde aplicável) salva, **Command-Z** desfaz e
**Command-Shift-Z** refaz edições confirmadas do projeto. Esses atalhos não
agem enquanto um campo de texto, janela modal ou menu suspenso detém a
entrada de teclado: apagar/desfazer no campo edita o texto, sem excluir nem
desfazer objetos da cena. Os atalhos passam pelas mesmas verificações de
disponibilidade e resolução de rascunhos dos controles visíveis. Salvar,
Desfazer e Refazer primeiro pedem uma decisão se houver um rascunho pendente;
Permanecer conserva o rascunho e não executa a ação. Fora disso, desfazer/refazer
atua sobre o histórico confirmado, não sobre o texto pendente.

## Projetos portáteis e sem conexão

Um `.pmcab` é um único arquivo JSON em UTF-8 com versão de formato explícita.
O esquema atual armazena materiais, peças, conjuntos, chapas, alocações,
preços das chapas, unidades de exibição, moeda, instâncias de ferragens e dados
de catálogo fixados no projeto, premissas de corte, articulações de portas e
relações de instalação.
Para reabrir e editar o projeto salvo, o outro computador não precisa de
conta, internet, caminhos de arquivo da máquina original nem baixar um
catálogo. Atualizações posteriores do catálogo não substituem silenciosamente
os dados fixados no projeto. Transfira **o arquivo `.pmcab` salvo**, não uma
cópia de recuperação ou um documento de corte exportado. O histórico de
desfazer vale somente na sessão atual: não acompanha o arquivo e recomeça
após a abertura ou recuperação.

### Transferência de arquivo entre computadores macOS arm64

1. No computador de origem, confirme as edições que deseja manter. Salve
   explicitamente o projeto em um arquivo `.pmcab` ou use Salvar como para
   escolher outro caminho `.pmcab`. Confirme o sucesso do salvamento e a
   ausência de alterações não salvas. O salvamento de recuperação não
   substitui esta etapa.
2. Copie esse arquivo para um pendrive ou outro meio de transferência e depois
   para uma pasta com permissão de escrita no computador de destino. Guarde
   uma cópia na origem até verificar a transferência. Basta o `.pmcab` para
   os dados de projeto descritos acima.
3. Na instalação macOS arm64 de destino, abra o `.pmcab` copiado com
   uma versão compatível do aplicativo. Confira o projeto, as alocações de
   chapas, os preços e os dados de ferragens fixados. Edite sem conexão e
   salve explicitamente a cópia local. Para levar essas edições de volta,
   repita o salvamento e a cópia no sentido inverso; as duas cópias não são
  sincronizadas.

O uso da interface no Linux e a transferência entre sistemas são experimentais
e não integram o suporte nem a validação desta primeira versão. Um teste de
transferência sem interface não comprova o funcionamento da aplicação Linux.

Cancelar Salvar como preserva o projeto, o caminho atual e o estado de
alterações não salvas. Se o salvamento falhar antes da substituição, o último
arquivo salvo com sucesso permanece intacto e as edições em memória continuam
não salvas; tente novamente ou escolha outro local com permissão de escrita.
Uma falha rara após a substituição, durante a sincronização do diretório, é
informada como **durabilidade incerta**: os novos dados podem já estar no
caminho, mas a edição não é marcada como salva. Verifique o arquivo antes de
depender dele ou transferi-lo. Não feche o projeto com alterações não salvas
pressupondo que uma tentativa ou um cancelamento de salvamento deu certo.

## Salvamento de recuperação e interrupções

### Tela inicial e projetos recentes locais

A tela inicial filtra projetos abertos ou salvos com sucesso **neste
computador**. Cada entrada recente apresenta somente metadados realmente
disponíveis: nome, caminho, contagens de peças/chapas, último uso e situação
qualificada de exportação/recuperação. Sem miniatura, aparece um substituto.
Caminhos e miniaturas são conveniências locais, não conteúdo portátil do
`.pmcab`. Uma entrada cujo arquivo está ausente oferece **Localizar…**, que
valida o substituto antes de alterar a entrada ou o projeto aberto, e
**Remover dos recentes**, que remove apenas a entrada—nunca o projeto salvo,
as cópias de recuperação nem as exportações. Voltar à tela inicial resolve
edições pendentes, mas mantém o documento atual sem pedir o salvamento das
alterações confirmadas. Abrir outro projeto recente ainda verifica edições
pendentes e alterações confirmadas não salvas antes de substituí-lo.
A prévia de um modelo ou uma tentativa de salvamento
falha não registra um projeto nos recentes.

Em Configurações → Geral é possível abrir a pasta de recuperação e
**Revisar cópias de recuperação**. A lista de limpeza inclui identidades
registradas, salvas ou sem arquivo, estado, datas conhecidas e registros
inválidos para diagnóstico. Nenhuma cópia é marcada automaticamente: escolha
cada uma para exclusão, confira os arquivos afetados e confirme explicitamente.
Cancelar mantém todas; uma falha parcial não apaga cópias não selecionadas nem
arquivos de projeto salvos. Não existe limpeza automática por idade nem busca
em pastas alheias do usuário.

A confirmação pelo teclado segue a ação que está visivelmente em foco.
As confirmações de limpeza e substituição começam em **Cancelar**; a recuperação
começa em **Decidir depois**. Enter nessa ação cancela/adia sem excluir,
substituir ou recuperar. Use Tab para chegar à ação afirmativa antes de pressionar
Enter. Escape cancela a confirmação atual; voltar da confirmação de limpeza
mantém a revisão, sem enviar o foco à área de trabalho por trás do diálogo.

Após 30 segundos sem uma nova edição confirmada, o aplicativo grava
uma cópia das alterações confirmadas e ainda não salvas. Uma nova
edição confirmada reinicia esse prazo de inatividade; prévias de digitação
ou arraste não confirmadas nunca são incluídas. A cópia fica separada, no **diretório de dados do usuário
do aplicativo** da plataforma (subpasta `recovery`), vinculada à identidade
do projeto e ao caminho do arquivo salvo. Ela é local à instalação, não faz
parte do `.pmcab`, e copiar ou renomear o projeto para outro caminho não
transfere nem associa a cópia de recuperação do caminho antigo.

A tela inicial mostra os candidatos de recuperação registrados. Para uma
cópia válida mais recente que o salvamento explícito, o cartão identifica
o UUID do projeto e o caminho canônico do arquivo, as revisões salva e
recuperável e, quando conhecida, a data da cópia. Metadados desconhecidos
continuam desconhecidos. Escolha explicitamente:

- **Recuperar edições:** continuar a partir do estado confirmado recuperado, ainda
  não salvo; salve explicitamente para substituir o arquivo salvo. O histórico
  de desfazer da sessão anterior não é restaurado.
- **Descartar cópia:** remover essa cópia de recuperação; o arquivo salvo
  explicitamente permanece inalterado.
- **Decidir depois:** manter a cópia e o arquivo salvo para decidir depois.

A recuperação nunca sobrescreve silenciosamente o salvamento explícito. Uma
cópia ausente, desatualizada, inválida ou de outro projeto/caminho não pode
substituir o projeto automaticamente; uma cópia inválida é mantida para
diagnóstico em vez de ser aceita como revisão válida. Faça salvamentos
explícitos para ter cópias portáteis.

Um cartão de recuperação **sem arquivo salvo** informa a identidade do
projeto sem inventar caminho ou revisão de salvamento. Recuperá-lo abre um
projeto não salvo que exige **Salvar como** para criar um `.pmcab` portátil.
Se outro documento permanecer aberto atrás da tela inicial, a recuperação
primeiro pede a decisão normal para edições pendentes e alterações não
salvas; cancelar preserva ambos os documentos e a cópia. **Decidir depois** mantém
a cópia para a próxima visita à tela inicial.

## Compatibilidade e arquivos danificados

A abertura verifica a versão do formato e valida o projeto inteiro antes de
substituir o atual. Esta versão lê as versões 1 e 2 do esquema; um arquivo da
versão 1 é validado e migrado **na memória**. A abertura não o regrava nem marca
o projeto como alterado. O primeiro Salvar ou Salvar como explícito pede
confirmação da atualização do esquema; Cancelar deixa os bytes originais
intactos. Confirmar grava a versão 2 de forma atômica, incluindo novos
metadados de aparência e procedência. Após
esse salvamento, um aplicativo antigo que aceite apenas a versão 1 não poderá
abrir o arquivo novo. Guarde uma cópia separada e intacta do arquivo da versão
1 se precisar voltar ao aplicativo antigo. Não há conversão destrutiva para a
versão anterior; abrir com uma versão antiga jamais deve remover os novos campos.
Confirmações antigas da largura de corte conservam o valor, mas têm **data
desconhecida**; somente uma nova confirmação explícita registra a data. Mudar
a largura de corte apaga tanto a confirmação quanto a data. Cores de exibição
dos materiais e etiquetas de estoque são portáteis na versão 2; a cor de
exibição ou sua ausência não altera quantidades de fabricação nem a condição
de liberação para corte.

O aplicativo recusa versões incompatíveis, inclusive arquivos gerados por uma
versão mais nova. Abra esses arquivos com uma versão compatível; uma versão antiga
não deve regravá-los. JSON incompleto/inválido, arquivos acima do limite de
16 MiB, referências quebradas, ciclos e valores inválidos impedem a abertura.
Em caso de falha, o arquivo de origem e o projeto atualmente aberto (inclusive
as alterações não salvas) permanecem intactos. Preserve o arquivo original
e restaure uma cópia íntegra ou corrija a origem fora do aplicativo antes de
tentar novamente.
