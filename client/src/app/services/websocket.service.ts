import { Injectable } from '@angular/core';
import { WebSocketSubject, WebSocketSubjectConfig } from 'rxjs/webSocket';
import { BehaviorSubject, Subject } from 'rxjs';
import { environment } from '../../environments/environment';

export type WebSocketAction = 'publish' | 'create' | 'subscribe';

export interface WebSocketMessage {
  action: WebSocketAction;
  topic: string;
  message?: string;    // Required for 'publish'
  password?: string; 
  sender?: string;  // Optional for 'create'
  timestamp?: string;
}

@Injectable({
  providedIn: 'root'
})
export class WebSocketService {
  private socket$!: WebSocketSubject<any>;
  private wsUrl = environment.wsUrl;
  public connected$ = new BehaviorSubject<boolean>(false);
  private messageSubject = new Subject<any>();
  private messagesSubject = new BehaviorSubject<WebSocketMessage>({} as WebSocketMessage);
  messages$ = this.messagesSubject.asObservable();

  connect(): WebSocketSubject<any> {
    if (!this.socket$ || this.socket$.closed) {
      const config: WebSocketSubjectConfig<any> = {
        url: this.wsUrl,
        openObserver: {
          next: () => {
            console.log('WebSocket connected');
            this.connected$.next(true);
          }
        },
        closeObserver: {
          next: () => {
            console.log('WebSocket disconnected');
            this.connected$.next(false);
          }
        }
      };
      
      this.socket$ = new WebSocketSubject(config);
      
      // Subscribe to incoming messages
      this.socket$.subscribe(
        (message) => {
          console.log('WebSocket received:', message);
          this.messagesSubject.next(message);
        },
        (error) => console.error('WebSocket error:', error)
      );
    }
    return this.socket$;
  }

  sendMessage(message: WebSocketMessage): void {
    if (this.socket$ && !this.socket$.closed) {
      console.log('Sending message:', message);
      this.socket$.next(message);
    } else {
      console.warn('WebSocket is not connected. Attempting to reconnect...');
      this.connect().next(message);
    }
  }

  disconnect() {
    if (this.socket$) {
      this.socket$.complete();
      this.connected$.next(false);
    }
  }

  private messageHandler(event: MessageEvent) {
    try {
      // First try to parse as JSON
      const data = JSON.parse(event.data);
      // Handle parsed JSON message
      this.messageSubject.next(data);
    } catch (e) {
      // If JSON parsing fails, handle as plain text
      console.log('Received non-JSON message:', event.data);
      // Handle as plain text message
      this.messageSubject.next({
        type: 'text',
        content: event.data
      });
    }
  }

  getMessages() {
    return this.messageSubject.asObservable();
  }

  private handleMessage(event: MessageEvent) {
    const message = JSON.parse(event.data);
    this.messagesSubject.next(message);
  }
} 