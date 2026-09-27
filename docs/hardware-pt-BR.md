# Referências de instalação de dobradiças

## Área de Ferragens

O cartão fixo do catálogo mostra os identificadores salvos do kit e do calço,
a revisão analisada e a fonte. **Explorar registros** abre as referências
guardadas no projeto, sem atualização automática nem necessidade de rede.
A árvore relaciona cada porta, a peça fixa e suas dobradiças; instalações sem
relação e ferragens dimensionadas de referência continuam acessíveis para
seleção, edição, posicionamento, duplicação e remoção. Um aviso acompanha a
instalação afetada, não outra dobradiça de nome parecido. A remoção de peças
relacionadas exige confirmação com os vínculos afetados.

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

Adicione ao catálogo do projeto o kit FGVTN Click 3D Slow Reta / H=0 `51MX153DRV00100` com calço `52MX15FG11003D`; registre uma instalação por dobradiça física. Escolha painéis distintos para a porta e a montagem, as bordas X de referência, as faces Z e os centros Y medidos desde a borda Y mínima de cada painel. Edições, exclusões e atualizações do catálogo podem ser desfeitas. Redimensionar o painel mantém as coordenadas e atualiza os avisos na lista.

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
