import { Component, OnInit, OnDestroy } from '@angular/core';
import { MatDialog } from '@angular/material/dialog';
import { Observable } from 'rxjs';
import { CreateRoomComponent } from './components/create-room/create-room.component';
import { WebSocketService } from './services/websocket.service';

@Component({
  selector: 'app-root',
  templateUrl: './app.component.html',
  styleUrls: ['./app.component.scss']
})
export class AppComponent implements OnInit, OnDestroy {
  isConnected$: Observable<boolean>;

  constructor(
    private dialog: MatDialog,
    private wsService: WebSocketService
  ) {
    this.isConnected$ = this.wsService.connected$;
  }

  ngOnInit() {
    // Establish WebSocket connection when component initializes
    this.wsService.connect().subscribe(
      message => {
        console.log('Received message:', message);
      },
      error => {
        console.error('WebSocket error:', error);
      }
    );
  }

  ngOnDestroy() {
    // Clean up the subscription when component is destroyed
    this.wsService.disconnect();
  }

  openCreateRoomDialog(): void {
    this.dialog.open(CreateRoomComponent, {
      width: '400px'
    });
  }
} 