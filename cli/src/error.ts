export class CliError extends Error {
  constructor(
    message: string,
    public readonly exitCode: number = 1,
  ) {
    super(message);
    this.name = 'CliError';
  }
}

export class NetworkError extends CliError {
  constructor(message: string) {
    super(message, 1);
    this.name = 'NetworkError';
  }
}

export class ApiError extends CliError {
  constructor(
    public readonly status: number,
    message: string,
  ) {
    super(`API error (HTTP ${status}): ${message}`, 1);
    this.name = 'ApiError';
  }

  static async fromResponse(resp: Response): Promise<ApiError> {
    let message: string;
    try {
      const body = await resp.json();
      message = body.message || body.error || resp.statusText;
    } catch {
      message = resp.statusText;
    }
    return new ApiError(resp.status, message);
  }
}

export class ConfigError extends CliError {
  constructor(message: string) {
    super(`Config error: ${message}`, 1);
    this.name = 'ConfigError';
  }
}
