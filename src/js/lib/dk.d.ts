declare const dk: {
  /** Log a message to stderr at info level. */
  log(...args: unknown[]): void;
  /** Log a message to stderr at warn level. */
  warn(...args: unknown[]): void;
  /** Log a message to stderr at error level. */
  error(...args: unknown[]): void;

  /** File system operations (async). */
  readonly fs: {
    /** Read a file's entire contents as a UTF-8 string. */
    read(path: string): Promise<string>;
    /** Write a string to a file, creating or overwriting it. */
    write(path: string, content: string): Promise<void>;
    /** Check whether a path exists. */
    exists(path: string): Promise<boolean>;
    /** Create a directory. Pass `{recursive: true}` to create parent dirs. */
    mkdir(path: string, opts?: { recursive?: boolean }): Promise<void>;
    /** Remove a file or directory. Pass `{recursive: true}` for non-empty dirs. */
    rm(path: string, opts?: { recursive?: boolean }): Promise<void>;
    /** List entries in a directory, returns filenames. */
    readDir(path: string): Promise<string[]>;
    /** Get file/directory metadata. */
    stat(path: string): Promise<{
      size: number;
      mtime: number;
      isDir: boolean;
      isFile: boolean;
      isSymlink: boolean;
    }>;
    /** Rename/move a file or directory. */
    rename(from: string, to: string): Promise<void>;
    /** Copy a file. */
    copy(from: string, to: string): Promise<void>;
  };

  /** Path manipulation (sync, no I/O). */
  readonly path: {
    /** Join path segments into a single path. */
    join(...parts: string[]): string;
    /** Return the parent directory of a path. */
    dirname(path: string): string;
    /** Return the last component of a path. */
    basename(path: string): string;
    /** Return the file extension including the leading dot (e.g. ".ts"). */
    extname(path: string): string;
    /** Resolve path segments against the current working directory. */
    resolve(...parts: string[]): string;
    /** Check if a path is absolute. */
    isAbsolute(path: string): boolean;
  };

  /** Environment variable access. */
  readonly env: {
    /** Get an environment variable's value, or `undefined` if unset. */
    get(key: string): string | undefined;
  };

  /** Process operations. */
  readonly process: {
    /** Execute a command and return its output. */
    exec(
      cmd: string,
      args?: string[],
    ): Promise<{ stdout: string; stderr: string; code: number }>;
    /** Get the current working directory. */
    cwd(): string;
    /** Exit the process with an optional code (default 0). */
    exit(code?: number): never;
    /** Command-line arguments. */
    readonly argv: string[];
    /** Current process ID. */
    readonly pid: number;
  };
};
