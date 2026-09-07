import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import {
  AppConfig,
  BackendEvent,
  BackendStatus,
  Capabilities,
  DiagnosticReport,
  StartRequest,
} from '../types/backend';

export class OziiBackend {
  static async getStatus(): Promise<BackendStatus> {
    return invoke<BackendStatus>('get_status');
  }

  static async start(request: StartRequest): Promise<void> {
    return invoke('start', { request });
  }

  static async stop(): Promise<void> {
    return invoke('stop');
  }

  static async recover(): Promise<boolean> {
    return invoke<boolean>('recover');
  }

  static async diagnose(): Promise<DiagnosticReport> {
    return invoke<DiagnosticReport>('diagnose');
  }

  static async getCapabilities(): Promise<Capabilities> {
    return invoke<Capabilities>('get_capabilities');
  }

  static async getConfig(): Promise<AppConfig> {
    return invoke<AppConfig>('get_config');
  }

  static async updateConfig(config: AppConfig): Promise<void> {
    return invoke('update_config', { config });
  }

  static async listenEvents(callback: (event: BackendEvent) => void) {
    return listen<BackendEvent>('backend-event', (e) => {
      callback(e.payload);
    });
  }
}
