
# Guia Definitivo do Pest Parser (Rust)

Este documento explica como ler, criar e dar manutenção em arquivos `.pest`.

O Pest usa **PEG (Parsing Expression Grammar)**, o que significa que ele é previsível: ele tenta as regras na ordem exata que você escreve e **não volta atrás** (sem *backtracking* oculto) a menos que você mande.

---

## 1. Os Operadores Básicos (A cola da gramática)

* `~` **(Sequência):** "Isso E DEPOIS aquilo".
  * *Exemplo:* `a ~ b` exige que venha "a" e logo em seguida "b".
* `|` **(Escolha):** "Isso OU aquilo".
  * *Exemplo:* `a | b`.
  * ⚠️ **CUIDADO:** A ordem importa! Ele tenta o da esquerda primeiro. Se o da esquerda der match, ele ignora o da direita. Coloque sempre os casos mais específicos **primeiro**.
* `*` **(Repetição Opcional):** Zero ou mais vezes (Pode não ter nada).
* `+` **(Repetição Obrigatória):** Uma ou mais vezes (Tem que ter no mínimo um).
* `?` **(Opcional):** Zero ou uma vez.

---

## 2. Os Modificadores de Regras (O segredo da AST limpa)

Antes de abrir as chaves `{ ... }`, você pode usar símbolos para dizer ao Pest como ele deve entregar essa informação pro seu código Rust:

### `{ ... }` (Normal)

A regra aparece na sua árvore no Rust. Os espaços em branco e comentários definidos em `WHITESPACE` e `COMMENT` são ignorados automaticamente entre os `~`.

### `_{ ... }` (Silenciosa / Silent)

O parser lê essa regra para validar o texto, mas ela **NÃO** é enviada para o Rust.

* *Uso ideal:* Ignorar formatações, chaves, parênteses e pontuações que não importam para a lógica.
* *Exemplo:* `_{ "{" }` valida que tem uma chave, mas não polui a sua AST.

### `@{ ... }` (Atômica / Atomic)

Desliga a ignorância de espaços em branco! Aqui dentro, um espaço é tratado como erro se não estiver explicitamente na regra.

* *Uso ideal:* Palavras-chave, nomes de variáveis (identificadores), onde `meu nome` não pode ser lido como `meunome`.

### `${ ... }` (Atômica Composta / Compound Atomic)

Parecido com a Atômica, mas se você chamar uma regra "Normal" lá dentro, aquela regra volta a ignorar espaços.

* *Uso ideal:* Interpolação de strings.

---

## 3. Regras Mágicas (Reservadas do Pest)

Se você declarar essas regras, o Pest muda o comportamento global do parser:

* `WHITESPACE` : Define o que é "espaço em branco". O Pest vai injetar isso silenciosamente entre todo `~` das regras normais.
  * *Exemplo:* `WHITESPACE = _{ " " | "\t" | "\r" | "\n" }`
* `COMMENT` : Define o que é comentário. Também é ignorado automaticamente.
  * *Exemplo:* `COMMENT = _{ "//" ~ (!"\n" ~ ANY)* }`
* `ANY` : Qualquer caractere único existente.
* `SOI` : *Start Of Input* (Começo do arquivo).
* `EOI` : *End Of Input* (Fim do arquivo).
  * *Uso ideal:* Colocar na sua regra principal (Root) para garantir que o parser leu o arquivo TODO e não parou no meio ao achar um erro.
  * *Exemplo:* `schema = { SOI ~ blocos* ~ EOI }`

---

## 4. Truques e Armadilhas (Boas Práticas)

### A. A Armadilha do "OU" Guloso

* ❌ **Errado:** `tipo = { ident | "table" }`
* ✅ **Certo:** `tipo = { "table" | ident }`
* *Motivo:* Se o texto for "table", a regra `ident` (que aceita letras) vai engolir a palavra antes de chegar na verificação literal da direita.

### B. Operador de Negação `!`

O Pest permite checar se algo NÃO está lá, sem consumir o caractere.

* *Exemplo:* `(!"\n" ~ ANY)*`
* *Lê-se:* "Enquanto o próximo caractere NÃO for uma quebra de linha, consuma qualquer caractere". Excelente para ler até o fim da linha (comentários) ou até fechar aspas (strings).

### C. Listas Separadas por Vírgula

Padrão ouro para ler coisas como `[a, b, c]`:

* *Exemplo:* `lista = { "[" ~ (item ~ ("," ~ item)*)? ~ "]" }`
* *Lê-se:* Abre colchete. Opcionalmente (`?`), tem um item principal, seguido de zero ou mais (`*`) sequências de "vírgula e outro item". Fecha colchete.

---

## 5. Como isso vai parar no Rust?

Quando você rodar `SchemaParser::parse(Rule::schema, texto)`, o Pest te devolve um iterador de **Pairs** (Pares), que representam os nós dessa árvore da imagem acima.

Cada "Pair" tem:

1. `.as_rule()` : Qual regra deu match (ex: `Rule::ident`, `Rule::table_block`).
2. `.as_str()`  : O texto exato que foi lido do arquivo (ex: `"User"`, `"String"`).
3. `.into_inner()`: Retorna os "filhos" daquela regra. Se você leu uma `table_block`, o `into_inner()` vai te dar um iterador com o nome da tabela e as declarações de campo de dentro dela.
4. `.as_span()`: Retorna a localização exata (linha e coluna) daquele token no código original (perfeito para mapear erros!).
