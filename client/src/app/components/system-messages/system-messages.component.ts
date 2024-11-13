import { Component, OnInit } from '@angular/core';
import { ChatService } from '../../services/chat.service';
import { Observable } from 'rxjs';
import { map } from 'rxjs/operators';
import { Message } from '../../models/room.model';

@Component({
  selector: 'app-system-messages',
  templateUrl: './system-messages.component.html',
  styles: [`
    .system-message {
      margin: 0.5rem;
      padding: 0.5rem;
    }
  `]
})
export class SystemMessagesComponent implements OnInit {
  systemMessages$: Observable<Message[]>;

  constructor(private chatService: ChatService) {
    this.systemMessages$ = this.chatService.getSystemMessages().pipe(
      map((messages: string[]) => messages.map((content: string) => ({
        content,
        timestamp: new Date(),
        type: 'system'
      })))
    );
  }

  ngOnInit() {}

  getSystemMessageClass(content: string): { [key: string]: boolean } {
    return {
      'message': true,
      'system-message': true,
      'info': content.includes('info'),
      'warning': content.includes('warning'),
      'error': content.includes('error')
    };
  }

  getSystemMessageIcon(content: string): string {
    if (content.includes('error')) return 'error';
    if (content.includes('warning')) return 'warning';
    if (content.includes('info')) return 'info';
    return 'notifications';  // default icon
  }
} 