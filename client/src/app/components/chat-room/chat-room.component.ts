import { Component, ViewChild, ElementRef, AfterViewChecked, OnInit, ChangeDetectorRef } from '@angular/core';
import { ChatService } from '../../services/chat.service';
import { Observable } from 'rxjs';
import { Room, Message } from '../../models/room.model';
import { NgClass, NgIf, DatePipe, NgFor, JsonPipe } from '@angular/common';
import { SafeHtmlPipe } from '../../pipes/safe-html.pipe';
import { AsyncPipe } from '@angular/common';
import { MatIconModule } from '@angular/material/icon';
import { MessageInputComponent } from '../../components/message-input/message-input.component';
import { tap } from 'rxjs/operators';

@Component({
  selector: 'app-chat-room',
  templateUrl: './chat-room.component.html',
  styleUrls: ['./chat-room.component.scss'],
  standalone: true,
  imports: [
    NgIf,
    NgFor,
    NgClass,
    AsyncPipe,
    DatePipe,
    JsonPipe,
    MatIconModule,
    MessageInputComponent,
    SafeHtmlPipe
  ]
})
export class ChatRoomComponent implements AfterViewChecked, OnInit {
  @ViewChild('messageContainer') messageContainer!: ElementRef;
  currentRoom$: Observable<Room | null>;
  
  constructor(
    public chatService: ChatService,
    private cdr: ChangeDetectorRef
  ) {
    this.currentRoom$ = this.chatService.getCurrentRoom().pipe(
      tap(room => {
        console.log('Current room updated:', room);
        setTimeout(() => {
          this.scrollToBottom();
          this.cdr.detectChanges();
        });
      })
    );
  }

  ngAfterViewChecked() {
    if (this.shouldScrollToBottom) {
      this.scrollToBottom();
      this.shouldScrollToBottom = false;
    }
  }

  ngOnInit() {
  }

  private shouldScrollToBottom = true;

  private scrollToBottom(): void {
    try {
      if (this.messageContainer) {
        const element = this.messageContainer.nativeElement;
        const isScrolledToBottom = element.scrollHeight - element.scrollTop <= element.clientHeight + 100;
        
        if (isScrolledToBottom) {
          element.scrollTop = element.scrollHeight;
        }
      }
    } catch (err) {
      console.error('Error scrolling to bottom:', err);
    }
  }

  trackByTimestamp(index: number, message: Message): Date {
    return message.timestamp;
  }

  getMessageClasses(message: Message): { [key: string]: boolean } {
    return {
      'message': true,
      'system-message': message.type === 'system',
      'user-message': message.type === 'user'
    };
  }

  getMessageIcon(message: Message): string {
    if (message.type === 'system') {
      return 'info';
    }
    return '';
  }

  onRoomChange(event: Event) {
    const target = event.target as HTMLSelectElement;
    const roomId = target.value;
    this.chatService.setCurrentRoom({ id: roomId } as Room);
  }

  handleMessage(roomId: string, event: any) {
    this.chatService.publishMessage(roomId, event.message || event);
  }
} 