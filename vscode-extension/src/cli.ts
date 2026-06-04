import { execFile } from "node:child_process";

export interface CliResult {
  stdout: string;
  stderr: string;
}

export function runNumCli(cliPath: string, args: string[]): Promise<CliResult> {
  return new Promise((resolve, reject) => {
    execFile(cliPath, args, { windowsHide: true }, (error, stdout, stderr) => {
      const result = { stdout, stderr };
      if (error) {
        reject(new NumCliError(error.message, result));
        return;
      }
      resolve(result);
    });
  });
}

export class NumCliError extends Error {
  constructor(message: string, readonly result: CliResult) {
    super(result.stderr || result.stdout || message);
    this.name = "NumCliError";
  }
}
