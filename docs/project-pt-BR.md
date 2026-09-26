# Arquivos de projeto e recuperação (português do Brasil)

## Disponibilidade

O aplicativo oferece Novo projeto, Abrir projeto, Salvar e Salvar como, com
indicador de alterações não salvas ao lado do nome/caminho. As janelas de
seleção nativas são assíncronas. Novo, Abrir e fechar pedem para salvar,
descartar ou cancelar alterações não salvas. Abrir valida o arquivo escolhido
antes de pedir para descartar o trabalho atual. Salvar como exige confirmação
antes de substituir um destino existente. O histórico de desfazer/refazer
recomeça para cada projeto aberto ou criado.

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

Após 30 segundos sem uma nova edição confirmada, o aplicativo grava
uma cópia das alterações confirmadas e ainda não salvas. Uma nova
edição confirmada reinicia esse prazo de inatividade; prévias de digitação
ou arraste não confirmadas nunca são incluídas. A cópia fica separada, no **diretório de dados do usuário
do aplicativo** da plataforma (subpasta `recovery`), vinculada à identidade
do projeto e ao caminho do arquivo salvo. Ela é local à instalação, não faz
parte do `.pmcab`, e copiar ou renomear o projeto para outro caminho não
transfere nem associa a cópia de recuperação do caminho antigo.

Se uma revisão de recuperação válida for mais recente que o salvamento
explícito, a reabertura apresenta o projeto/caminho e as
revisões salva e recuperável para escolha explícita:

- **Recuperar:** continuar a partir do estado confirmado recuperado, ainda
  não salvo; salve explicitamente para substituir o arquivo salvo. O histórico
  de desfazer da sessão anterior não é restaurado.
- **Descartar:** remover essa cópia de recuperação; o arquivo salvo
  explicitamente permanece inalterado.
- **Adiar:** manter a cópia e o arquivo salvo para decidir depois.

A recuperação nunca sobrescreve silenciosamente o salvamento explícito. Uma
cópia ausente, desatualizada, inválida ou de outro projeto/caminho não pode
substituir o projeto automaticamente; uma cópia inválida é mantida para
diagnóstico em vez de ser aceita como revisão válida. Faça salvamentos
explícitos para ter cópias portáteis.

## Compatibilidade e arquivos danificados

A abertura verifica a versão do formato e valida o projeto inteiro antes de
substituir o atual. Este núcleo aceita a versão 1 do esquema e recusa versões
incompatíveis, inclusive arquivos gerados por uma versão mais nova do
aplicativo. Abra esses arquivos com uma versão compatível; uma versão antiga
não deve regravá-los. JSON incompleto/inválido, arquivos acima do limite de
16 MiB, referências quebradas, ciclos e valores inválidos impedem a abertura.
Em caso de falha, o arquivo de origem e o projeto atualmente aberto (inclusive
as alterações não salvas) permanecem intactos. Preserve o arquivo original
e restaure uma cópia íntegra ou corrija a origem fora do aplicativo antes de
tentar novamente.
