# Revestimento e aparência das chapas

Chapas de MDF, MDP e HDF são vendidas **revestidas** (BP, laminado) nas duas
faces, em uma face, ou cruas. O Plan My Cabinet guarda isso no material, e a
área 3D mostra.

## O revestimento é do material

Escolha **Revestimento** (*Nenhum (cru)*, *Uma face*, *Duas faces*) na janela
do material; ele aparece para MDF, MDP e HDF. Como faz parte do material,
"MDF Branco 1 face" e "MDF Branco" são dois materiais: cada um tem suas
chapas no Estoque, e o plano de corte só corta uma peça de chapa do mesmo
material. Arquivos anteriores ao revestimento o recebem pelo nome do
material: "Cru" ou "Raw" é sem revestimento, "1 face", "uma face" ou "one
side" é uma face, e o resto, duas faces.

## Qual face é revestida

Num material revestido em uma face, cada peça mostra o revestimento onde ele
aparece. Automaticamente:

- peças atravessadas no móvel (portas, frentes, fundos) são revestidas para a
  frente;
- peças deitadas (bases, prateleiras, tampos) são revestidas em cima;
- peças ao longo do móvel (laterais, divisórias) são revestidas por fora.

A seção **Revestimento** do inspetor diz qual face é revestida e por quê.
**Virar face revestida** escolhe a outra face à mão; **Voltar ao automático**
volta à regra. Cada um é um passo de desfazer, e funciona com várias peças
selecionadas. A face nunca muda de qual chapa a peça é cortada.

## O que a área 3D mostra

- Faces revestidas: a cor do material.
- Faces cruas e bordas sem fita: o miolo da chapa, com sua textura: fibra de
  MDF, partículas de MDP, fibra de HDF, lâminas de compensado ou veios de
  madeira.
- Bordas com fita: a cor da fita (veja [Fita de borda](banding-pt-BR.md)).

Faces de compensado e madeira maciça mostram veios na cor do material. Com
*Colorir peças pela cor do material* desligado, todas as faces ficam neutras.
