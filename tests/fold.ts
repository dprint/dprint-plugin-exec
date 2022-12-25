// re-implementation of the unix "fold" command so it runs on Windows

const args = parseArgs(Deno.args);
const fileText = args.filePath == null
  ? await readStdin()
  : await Deno.readTextFile(args.filePath);

console.log(limitLinesToWidth(
  paragraphsToSingleLines(fileText),
  args.lineWidth,
));

function paragraphsToSingleLines(text: string) {
  text = text.trim();
  let finalText = "";
  let previousLine: string | undefined = undefined;
  for (const line of text.split(/\r?\n/g).map((l) => l.trim())) {
    if (line.length === 0) {
      finalText += "\n\n";
    } else if (previousLine == null || previousLine.length === 0) {
      finalText += line;
    } else {
      finalText += " " + line;
    }
    previousLine = line;
  }
  return finalText;
}

function limitLinesToWidth(fileText: string, width: number) {
  let finalText = "";
  let lineLength = 0;
  for (const token of tokenize(fileText)) {
    if (token.kind === "newline") {
      finalText += "\n";
      lineLength = 0;
    } else if (token.kind === "word") {
      if (lineLength + token.text.length > width) {
        finalText += "\n";
        lineLength = 0;
      }
      if (lineLength > 0) {
        finalText += " ";
        lineLength++;
      }
      finalText += token.text;
      lineLength += token.text.length;
    }
  }

  return finalText;
}

type Token = Word | NewLine;

interface Word {
  kind: "word";
  text: string;
}

interface NewLine {
  kind: "newline";
}

function* tokenize(text: string): Iterable<Token> {
  text = text.trim();
  let wordStart = 0;
  for (let i = 0; i < text.length; i++) {
    if (text[i] === "\n" || text[i] === " ") {
      const word = text.slice(wordStart, i).trim();
      if (word.length > 0) {
        yield { kind: "word", text: word };
      }
      if (text[i] === "\n") {
        yield { kind: "newline" };
      }
      wordStart = i + 1;
    }
  }

  const word = text.slice(wordStart).trim();
  if (word.length > 0) {
    yield { kind: "word", text: word };
  }
}

async function readStdin() {
  const readable = Deno.stdin.readable.pipeThrough(new TextDecoderStream());
  let finalText = "";
  for await (const chunk of readable) {
    finalText += chunk;
  }
  return finalText;
}

interface Args {
  lineWidth: number;
  filePath: string | undefined;
}

function parseArgs(args: string[]): Args {
  // super super basic so it works in the tests
  let i = 0;
  let lineWidth = 80;
  if (args[i] === "-w") {
    lineWidth = parseInt(args[++i], 10);
  }
  const filePath = args[++i];
  if (args[i + 1] != null) {
    throw new Error("Invalid");
  }
  return {
    lineWidth,
    filePath,
  };
}
