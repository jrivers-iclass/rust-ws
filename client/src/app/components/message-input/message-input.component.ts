import { Component, Input, OnInit, OnDestroy } from '@angular/core';
import { ReactiveFormsModule, FormControl } from '@angular/forms';
import { MatFormFieldModule } from '@angular/material/form-field';
import { MatInputModule } from '@angular/material/input';
import { MatIconModule } from '@angular/material/icon';
import { MatButtonModule } from '@angular/material/button';
import { ChatService } from '../../services/chat.service';
import { FormsModule } from '@angular/forms';
import { Subscription } from 'rxjs';
import { take } from 'rxjs/operators';

@Component({
  selector: 'app-message-input',
  templateUrl: './message-input.component.html',
  styleUrls: ['./message-input.component.scss'],
  standalone: true,
  imports: [
    ReactiveFormsModule,
    MatFormFieldModule,
    MatInputModule,
    MatIconModule,
    MatButtonModule,
    FormsModule
  ]
})
export class MessageInputComponent implements OnInit, OnDestroy {
  @Input() roomId!: string;
  messageControl = new FormControl('');
  private subscriptions = new Subscription();

  constructor(private chatService: ChatService) {}

  ngOnInit() {
    // Initialize any necessary subscriptions
  }

  ngOnDestroy() {
    this.subscriptions.unsubscribe();
  }

  sendMessage() {
    const message = this.messageControl.value?.trim();
    if (message && this.roomId) {
      this.chatService.publishMessage(this.roomId, message);
      this.messageControl.reset();
    }
  }

  onKeyPress(event: KeyboardEvent) {
    if (event.key === 'Enter' && !event.shiftKey) {
      event.preventDefault();
      this.sendMessage();
    }
  }
} 