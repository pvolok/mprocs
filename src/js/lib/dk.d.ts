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
  };

  /** Environment variable access. */
  readonly env: {
    /** Get an environment variable's value, or "" if unset. */
    get(key: string): string;
    /** Set an environment variable. */
    set(key: string, value: string): void;
  };
};
