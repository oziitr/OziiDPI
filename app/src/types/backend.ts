export type BackendStatus =
  | 'Disconnected'
  | 'Starting'
  | 'Connected'
  | 'Stopping'
  | 'Recovering'
  | 'Faulted';

export type DpiMode =
  | { type: 'turbo' }
  | { type: 'balanced'; chunk_size: number }
  | { type: 'strong' }
  | { type: 'strong_advanced'; fake_count: number };

export interface StartRequest {
  mode: DpiMode;
}

export interface RoutingMetrics {
  discord_forwarded: number;
  non_discord_rejected: number;
  non_discord_forwarded: number;
  discord_failed: number;
}

export interface ActivePorts {
  engine: number;
  adapter: number;
  pac: number;
  diagnostics: number;
}

export interface DiagnosticReport {
  app_version: string;
  engine_version: string;
  windows_version: string;
  mode: string;
  backend_state: BackendStatus;
  pac_active: boolean;
  active_ports: ActivePorts | null;
  routing_metrics: RoutingMetrics;
  error_codes: string[];
  recovery_status: string;
  updater_redirect_active: boolean;
}

export interface Capabilities {
  strong_advanced_available: boolean;
  npcap_available: boolean;
  engine_version: string;
  browser_routing_supported: boolean;
  discord_desktop_supported: boolean;
  recovery_required: boolean;
}

export interface AppConfig {
  mode: string;
  chunk_size: number;
  fake_count: number;
  ignore_wpad: boolean;
  protect_discord_updater: boolean;
}

export interface BackendEvent {
  StateChanged?: {
    old_state: BackendStatus;
    new_state: BackendStatus;
  };
  EngineStarted?: {
    pid: number;
  };
  EngineFailed?: {
    reason: string;
  };
  RoutingMetricsUpdated?: {
    metrics: unknown;
  };
  DiagnosticWarning?: {
    message: string;
  };
  RecoveryStarted?: null;
  RecoveryFinished?: {
    success: boolean;
  };
}
