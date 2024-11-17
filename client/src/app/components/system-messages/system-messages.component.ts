import { Component, OnInit } from '@angular/core';
import { Observable } from 'rxjs';
import { map } from 'rxjs/operators';
import { ChatService } from '../../services/chat.service';
import { Message } from '../../models/room.model';

@Component({
  selector: 'app-system-messages',
  templateUrl: './system-messages.component.html',
  styleUrls: ['./system-messages.component.scss']
})
export class SystemMessagesComponent implements OnInit {
  systemMessages$: Observable<Message[]>;

  constructor(private chatService: ChatService) {
    this.systemMessages$ = this.chatService.getSystemMessages().pipe(
      map((messages: Message[]) => messages.map((message: Message) => ({
        content: message.content,
        timestamp: new Date(),
        type: 'system' as const,
        sender: 'System'
      })))
    );
  }

  ngOnInit() {}

  getSystemMessageClass(content: string): { [key: string]: boolean } {
    return {
      'system-message': true,
      'error': content.toLowerCase().includes('error'),
      'warning': content.toLowerCase().includes('warning'),
      'info': !content.toLowerCase().includes('error') && !content.toLowerCase().includes('warning')
    };
  }

  getSystemMessageIcon(content: string): string {
    if (content.toLowerCase().includes('error')) return 'error';
    if (content.toLowerCase().includes('warning')) return 'warning';
    return 'info';
  }
} 