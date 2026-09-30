# Entrega à oficina e revisões exportadas

A área Entrega abre a prévia paginada em A4 ao lado dos controles do pacote e
do histórico separado de exportações. Escolha uma miniatura de página, use
Anterior/Próxima ou amplie sem mudar a paginação do exemplo de seis páginas.
Prévia e PDF usam o mesmo documento posicionado e as fontes incluídas no
aplicativo; não dependem de fontes pela internet nem de visualizador externo
para funcionar. Com a prévia pequena, confira o PDF em tamanho de impressão
antes de enviá-lo: miniaturas não são instruções de corte.

A página começa ajustada à área disponível, inclusive após redimensionar a janela
ou mudar a escala da interface. Alterar Zoom passa à ampliação manual; use
**Ajustar página** para mostrar a página inteira novamente. Páginas ampliadas
rolam independentemente das miniaturas. Ajustar e ampliar não mudam as dimensões
nem o conteúdo do PDF.

## Revisão e seções opcionais

O cartão de preparo oferece ações **Corrigir na área de trabalho** para cada
pendência. Uma peça oculta e não alocada também impede o modo pronto; preço ou
tarifa desconhecidos tornam a estimativa incompleta, mas não invalidam por si
só um plano de madeira verificado. Lista de peças e custos, Diagramas de
chapas e etapas de corte, e Referências de dobradiças começam incluídos.
Desativar uma seção retira somente detalhes opcionais: identificação,
premissas, omissões, pendências de madeira e ferragens, custos incompletos e
avisos de segurança permanecem. Diagramas e suas etapas são uma escolha
indivisível. Miniaturas e contagem de páginas A4 vêm do documento preparado;
navegar ou ampliar a prévia não refaz a paginação.

Depois de mudar o projeto de origem ou *qualquer* opção de saída (modo, idioma,
unidades, seções), espere as páginas atualizadas e selecione explicitamente
**Revisar esta prévia**. A exportação permanece indisponível até reconhecer
exatamente esse pacote. Um seletor de arquivo retornando depois de uma mudança
não pode gravar o pacote antigo nem outro recalculado sem revisão. Cancelar o
seletor mantém uma revisão ainda atual; recusar substituição ou falhar ao gravar
não cria comprovante de sucesso. Um comprovante concluído registra modo,
seções, idioma, unidades, arquivo, horário efetivo e hash. Os cartões históricos
distinguem substituição por outro pacote da atualidade do conteúdo; detalhes
de **Desde então** exigem comparação registrada. Comprovantes antigos sem
modo, data ou base de comparação identificam esses dados como indisponíveis,
sem adivinhação. Alterar apenas a cor de um material pode deixar o projeto
não salvo sem desatualizar o conteúdo de fabricação.

1. Conclua a montagem e identifique cada peça física pelo nome **e ID individual**. Prateleiras duplicadas têm IDs distintos mesmo quando a lista agrupa suas dimensões por quantidade; compare os IDs na lista de objetos, na posição da chapa e na chave de peças do PDF. Aloque também peças ocultas. Confira material, espessura medida do estoque, veio, origem do estoque, refilos e preços. Depois de configurar, confirme no projeto a largura real da lâmina da oficina; alterá-la remove a confirmação. Confira a prática de refilo, a capacidade de cortar integralmente cada peça retangular, a premissa de não empilhar, a fixação e a tarifa fixa por corte físico. O modelo não certifica segurança nem viabilidade prática da operação.
2. Escolha explicitamente **Rascunho** para análise pendente ou **Pronto para a oficina** para o corte. As páginas do rascunho trazem **RASCUNHO / NÃO USAR PARA CORTE** e as pendências. O modo pronto exige largura de corte atual confirmada, alocações compatíveis de *todas* as peças e sequência de cortes integrais verificada para cada chapa utilizada. Resolva peças sem alocação, conflitos e buscas sem prova antes de tentar novamente; preços ou tarifa desconhecidos indicam estimativa incompleta, mas não impedem cortes viáveis. Campo vazio é **desconhecido**, nunca zero; `0` informado é zero conhecido. Impostos, entrega, preparação e descontos por empilhamento não entram no custo.
3. Escolha **idioma e unidades do PDF independentemente** do idioma da interface e da unidade exibida no projeto; confira moeda e medidas convertidas. Nomes digitados e IDs não são traduzidos. Confira cada resumo de estoque, dimensão final do retângulo, seta de veio, refilo e corte numerado **C1, C2…**. Na legenda, **P0, P1…** identificam peças intermediárias/de entrada/de saída, não o ID da peça do projeto; cada peça final remete ao seu ID completo. Execute na ordem numérica: tome a peça P de entrada indicada, use a borda de referência X-/X+ ou Y-/Y+, meça a dimensão retida, conserve a saída indicada no lado especificado e respeite a largura de corte separada no lado apontado. Posição da serra não é dimensão final da peça. Conte uma passagem por divisão, inclusive refilos. Confirme com a oficina ordem física, lado da lâmina, veio e manuseio. Os desenhos declaram escala e **não são gabaritos de corte**: revise manualmente números e desenhos, sem medir o papel como modelo.
4. Escolha o destino. Cancelar não cria PDF nem comprovante de sucesso; substituir arquivo existente exige confirmação explícita. Para uma revisão já enviada, prefira outro nome de arquivo e preserve o pacote anterior. Após a gravação concluída, o comprovante registra ID do projeto, revisão do documento, impressões digitais da madeira/do pacote, idioma, unidades, caminho, horário e hash do PDF. Falha na gravação não gera comprovante de sucesso. Salve o projeto `.pmcab` para persistir o registro.
5. Após mudar nome, dimensão, estoque, largura de corte, alocação ou preço, confira o estado **desatualizado** em relação ao último comprovante; alterar ferragens incluídas desatualiza o pacote separadamente da viabilidade da madeira. Uma nova revisão do documento por si só (por exemplo a grade de edição) não desatualiza a impressão digital de fabricação; câmera e visibilidade também não. Edições posteriores não alteram o PDF já exportado: não o envie como plano atual. Resolva novos conflitos, confirme novamente a largura de corte alterada, prepare uma nova versão pronta e exporte para outro caminho; registre e salve o novo comprovante. Trocar idioma/unidades do PDF requer outra exportação explícita, não reescreve automaticamente o arquivo anterior.

Para montar um exemplo com peças individuais, sem modelo pronto de armário, veja [Conjuntos e hierarquia](assembly-pt-BR.md). Para primeira posição viável, largura de corte e estimativa, veja [Estoque do projeto](stock-pt-BR.md).

## Confirmação da espessura de corte

A ação em Entrega, Configurações → Corte ou na paleta de comandos abre um diálogo
de revisão; abri-lo não confirma nada. Confira o valor exato exibido com a oficina,
marque a declaração e escolha **Confirmar espessura**. Enter na caixa apenas a
marca/desmarca. Cancelar ou Escape mantém o projeto intacto; se o projeto mudar,
reabra a revisão. A confirmação registra a data de hoje para esse valor em uma
edição que pode ser desfeita. Não certifica o plano nem a segurança do corte.
Alterar a espessura de corte remove a confirmação.

## Outros formatos

A escolha **Formato** da Entrega também oferece o **CorteCloud**, uma lista de
peças para pedir peças cortadas, com fita e furadas a uma marcenaria. Ele não
precisa de revisão nem de plano de corte; veja
[Pedindo peças pelo CorteCloud](cortecloud-pt-BR.md). A lista de peças do PDF
mostra a fita de borda de cada peça e a fita a comprar; veja
[Fita de borda](banding-pt-BR.md).
