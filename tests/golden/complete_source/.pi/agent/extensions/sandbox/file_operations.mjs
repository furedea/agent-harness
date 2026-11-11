// Untrusted plumbing: this process and every child must run inside srt.
import { access, mkdir, readFile, writeFile } from 'node:fs/promises';
import { constants } from 'node:fs';

async function execute(request) {
  switch (request.operation) {
    case 'readFile': return (await readFile(request.path)).toString('base64');
    case 'writeFile': await writeFile(request.path, request.content); return null;
    case 'mkdir': await mkdir(request.path, { recursive: true }); return null;
    case 'access': await access(request.path, request.write ? constants.R_OK | constants.W_OK : constants.R_OK); return null;
    case 'detectImageMimeType': {
      const bytes = await readFile(request.path);
      if (bytes.subarray(0, 8).equals(Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]))) return 'image/png';
      if (bytes[0] === 255 && bytes[1] === 216 && bytes[2] === 255) return 'image/jpeg';
      if (['GIF87a', 'GIF89a'].includes(bytes.toString('ascii', 0, 6))) return 'image/gif';
      if (bytes.toString('ascii', 0, 4) === 'RIFF' && bytes.toString('ascii', 8, 12) === 'WEBP') return 'image/webp';
      return null;
    }
    case 'search': {
      const pi = await import(process.argv[2]);
      const factories = { grep: pi.createGrepTool, find: pi.createFindTool, ls: pi.createLsTool };
      const factory = factories[request.tool];
      if (!factory) throw new Error('Unknown search tool');
      return factory(process.cwd()).execute(request.id, request.params);
    }
    default: throw new Error('Unknown file operation');
  }
}

try {
  let input = '';
  for await (const chunk of process.stdin) input += chunk;
  process.stdout.write(JSON.stringify({ value: await execute(JSON.parse(input)) }));
} catch (error) {
  process.stdout.write(JSON.stringify({ error: { message: error.message, code: error.code } }));
  process.exitCode = 1;
}
