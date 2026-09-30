# Pedindo peças pelo CorteCloud

O [CorteCloud](https://cortecloud.com) permite que uma marcenaria corte,
coloque fita e fure suas peças com o estoque dela. O Plan My Cabinet grava a
lista de peças que o CorteCloud importa, para você não digitar de novo.

## Exportando

1. Abra **Entrega** e escolha **CorteCloud** em *Formato*.
2. Confira **O que o arquivo lista**: peças e linhas (peças iguais dividem uma
   linha com quantidade), peças com fita e metros de fita, peças furadas e
   furos, e as peças por material.
3. Leia **Ficou de fora**. Os furos só vão para ferragens que o PDF da oficina
   também orientaria:
   - dobradiças ou corrediças com problemas, e portas que precisam de revisão,
     ficam de fora (**Corrigir** leva até elas);
   - furos de parafuso sem medida de pré-furo ficam de fora. Os catálogos
     raramente dão o pré-furo de calços e corrediças; marque **Pedir à
     marcenaria os pré-furos de parafuso** e informe diâmetro e profundidade
     para incluí-los.
4. Toque em **Exportar para o CorteCloud…** e escolha onde salvar. Um arquivo
   que já existe só é substituído depois da sua confirmação.

O centro da Entrega mostra as peças exatamente como o arquivo lista. A
exportação só precisa de um projeto válido: chapas, plano de corte, espessura
da serra e preços não são usados, porque a marcenaria encaixa as peças nas
chapas dela. Exportar não é uma edição; a última exportação fica registrada e a
Entrega avisa quando o projeto mudou desde então.

## Importando no CorteCloud

No CorteCloud: **Novo serviço › Serviço Completo › Carregar arquivo
Cortecloud**, escolha o arquivo e vincule cada material e fita ao estoque da
marcenaria. Confira a prévia das primeiras peças que importar: furos e fitas
devem estar onde o Plan My Cabinet mostra.

## O que vai no arquivo

| Campo do CorteCloud | Do projeto |
|---|---|
| `c` × `l` | Comprimento e largura finais. `c` segue o veio; sem regra de veio, o lado maior. |
| `quantity` | Quantas peças iguais: mesmo nome, móvel, material, medida, veio, fita e furos. |
| `function` | O nome da peça. |
| `complement` | O móvel (conjunto de cima) a que a peça pertence. |
| `material` | Nome e espessura do material, por exemplo "MDF Branco 18". |
| `c1` `c2` `l1` `l2` | A fita de cada lado, pelo nome, ou vazio. |
| `machining` | Furos de face: copos de dobradiça e, com medida, pré-furos de parafuso, cada um a partir do canto mais próximo, com profundidade, diâmetro e se é passante. Peças sem furos não têm usinagem. |

A face com mais furos é a face interna (`i`). Furos de topo, rasgos e
rebaixos ainda não são modelados, então o arquivo não tem nenhum.

## Para agentes

As ferramentas MCP `get_part_list` e `export_design` fazem o mesmo; veja o guia
do agente.
