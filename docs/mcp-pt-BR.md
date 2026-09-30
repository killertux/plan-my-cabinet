# Usar o Plan My Cabinet com um agente de IA (MCP)

O Plan My Cabinet pode funcionar como um servidor
[Model Context Protocol](https://modelcontextprotocol.io). Um agente como o
Claude então projeta móveis com as mesmas edições verificadas do aplicativo:
cria as peças, declara o estoque, planeja os cortes, adiciona dobradiças e
portas, vê imagens do resultado e salva um arquivo `.pmcab` que você abre no
aplicativo.

O servidor não tem janela. Ele roda no seu computador e só lê e grava os
arquivos que o agente for instruído a abrir ou salvar.

## Configuração

Use o programa instalado com a opção `--mcp`.

**Claude Code**:

```sh
claude mcp add plan-my-cabinet -- "/Users/voce/Applications/Plan My Cabinet.app/Contents/MacOS/plan-my-cabinet" --mcp
```

No Linux o programa é `~/.local/bin/plan-my-cabinet`; no Windows,
`%LOCALAPPDATA%\Programs\PlanMyCabinet\plan-my-cabinet.exe`.

**Claude Desktop** (`claude_desktop_config.json`):

```json
{
  "mcpServers": {
    "plan-my-cabinet": {
      "command": "/Users/voce/Applications/Plan My Cabinet.app/Contents/MacOS/plan-my-cabinet",
      "args": ["--mcp"]
    }
  }
}
```

Opções:

| Opção | Significado |
|---|---|
| `--open ARQUIVO.pmcab` | Abre um projeto ao iniciar. |
| `--catalog-dir PASTA` | Pasta com seus pacotes de catálogo de dobradiças. Padrão: a pasta `catalogs` do aplicativo. |
| `--user-data-dir PASTA` | Pasta de dados do aplicativo (projetos recentes, catálogos). Projetos salvos aparecem na tela inicial. |
| `--language en\|pt-BR` | Idioma dos nomes dos materiais padrão. |

## O que o agente pode fazer

Cerca de setenta ferramentas, em grupos:

- **Projeto**: novo, abrir, salvar, fechar, configurações (unidades, largura de corte, taxa de corte), moeda, desfazer e refazer.
- **Projeto do móvel**: materiais, peças (criar, copiar, redimensionar, mover, girar, encostar uma na outra), montagens, ferragens de referência e os modelos Base, Aéreo e Gaveteiro.
- **Ver o resultado**: uma descrição escrita do modelo (posições, peças que se tocam e peças que se **sobrepõem**), imagens de qualquer ângulo com peças ocultas, isoladas ou destacadas, desenhos do plano de corte de cada chapa e uma porta aberta.
- **Estoque e plano de corte**: chapas e retalhos, chapas que faltam, posicionamento automático e manual, diagnóstico e otimização do plano.
- **Ferragens**: catálogos de dobradiças, dobradiças nas portas e portas que abrem.

A exportação do PDF para a oficina ainda não está disponível: abra o arquivo
salvo no aplicativo e use **Entrega**.

## Peça algo

Por exemplo:

> Projete um balcão de cozinha com 800 mm de largura, 720 mm de altura e
> 580 mm de profundidade, com duas portas. Compre as chapas necessárias a
> R$ 320 cada, otimize o plano de corte, adicione as dobradiças, mostre a
> frente e o interior sem as portas e salve em ~/Documentos/balcao.pmcab.

O agente trabalha em etapas e confere cada uma. Ele pergunta antes de
sobrescrever um arquivo ou descartar alterações não salvas.

## Bom saber

- Os números são milímetros. O agente também pode escrever `"60 cm"` ou
  `"23 5/8 in"`. Valores que não caem em um micrômetro inteiro precisam do
  consentimento explícito dele.
- Cada alteração é um passo de desfazer enquanto o servidor roda; o arquivo
  no disco só muda ao salvar.
- As posições das dobradiças são referências do catálogo, como no aplicativo.
  Confira com a folha do fabricante antes de furar.
- O servidor não imprime nada na saída padrão além das mensagens do
  protocolo. Problemas vão para a saída de erro.
